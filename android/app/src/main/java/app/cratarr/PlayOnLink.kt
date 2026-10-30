package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.Player
import org.json.JSONException
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.util.concurrent.Executors

/**
 * Play On, the app's side (decision 0031, step 3): this player's connection to Cratarr. It keeps
 * `GET /api/players/{name}/connect` open (server-sent events: the commands for it), runs each command on the app's own
 * player, and reports what it plays with `PUT /api/players/{name}/state`: on every change, and every 30 s besides.
 * Cratarr counts the player online while the connection is open. With Stay ready, a wake lock and a Wi-Fi lock keep
 * the connection up while nothing plays.
 */
class PlayOnLink(
    private val service: PlayerService,
    private val player: Player,
    private val prefs: Prefs,
    private val sync: SyncPlay,
    /** The player's commands (one way in, the page's too). */
    private val commands: PlayerCommands,
) : Player.Listener {
    private val main = Handler(Looper.getMainLooper())
    private val reports = Executors.newSingleThreadExecutor()
    private val wakeLock = service.getSystemService(PowerManager::class.java)
        .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cratarr:ready")
        .apply { setReferenceCounted(false) }

    @Suppress("DEPRECATION") // the mode that keeps Wi-Fi up with the screen off
    private val wifiLock = (service.applicationContext.getSystemService(android.content.Context.WIFI_SERVICE) as WifiManager)
        .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "cratarr:ready")
        .apply { setReferenceCounted(false) }

    @Volatile private var running = false
    @Volatile private var connection: HttpURLConnection? = null
    private var thread: Thread? = null

    /**
     * Its volume is Android's media volume (the owner, 2026-10-23: a volume of its own, set by Play On and seen nowhere on
     * the device, left the phone silent at full volume): Play On's `volume` sets it, and the reports say it, so the
     * system's slider, the volume keys, Home Assistant and the page move one and the same. The player itself plays at
     * the duck times the track's levelling only.
     */
    private val audio = service.getSystemService(android.media.AudioManager::class.java)
    /** The media volume changed (the keys, the slider, Play On): reported. */
    private val volumeSeen = object : android.database.ContentObserver(android.os.Handler(android.os.Looper.getMainLooper())) {
        override fun onChange(selfChange: Boolean) = soon()
    }
    private var ducked = false
    /** The announcement it plays or last played (Announcer), as reported. */
    private var announcing: Announcer.State? = null
    private var queueSent = false
    /** The queue's version in the last command from Cratarr: reports say it, so Cratarr knows what they have seen. */
    private var version = -1L
    private var reportPending = false

    // The link as this player sees it (decision 0045): written to its own log, and told in its reports.
    /** Connected since (epoch ms, this device's clock); 0 while not. */
    @Volatile private var connectedSince = 0L
    /** Attempts that failed since it was last connected, and the last one's error. */
    @Volatile private var failed = 0
    @Volatile private var lastError: String? = null
    /** A wait before trying again: until when (epoch ms, this device's clock; 0: none), and why. */
    @Volatile private var waitUntil = 0L
    @Volatile private var waitWhy: String? = null
    /** Why the app itself is closing the connection now (another address, stopping…); null: it is not. */
    @Volatile private var closing: String? = null
    /** Whether the last report got through: a change either way is written down, not every report. */
    @Volatile private var reportsThrough = true

    // A start Android refuses (decision 0031, amended 2026-10-19): an app gets the sound only on the screen or with a
    // foreground service, and the player pauses at once without it.
    /** When Cratarr last asked it to play (elapsed ms): a pause for want of the sound right after is a refusal. */
    private var askedAt = 0L
    /** Asked again already, after the first refusal. */
    private var retried = false
    /** The play asked for again once it is in the foreground; cancelled by a pause or a stop. */
    private var again: Runnable? = null
    /** Why it does not play what it was told to (Android would not let it), and since when (elapsed ms); null: nothing
     *  holds it back. Reported to Cratarr; the music starts as the app comes to the front, or by the notification. */
    private var heldBack: String? = null
    private var heldAt = 0L

    /** Where the sound goes: a change is reported at once (decision 0033, amended 2026-10-19). */
    private var outputs: Output.Watch? = null

    fun start() {
        if (running) return
        running = true
        outputs = Output.Watch(service) {
            service.outputsChanged()
            soon()
        }
        service.contentResolver.registerContentObserver(android.provider.Settings.System.CONTENT_URI, true, volumeSeen)
        player.addListener(this)
        thread = Thread(::listen, "cratarr-play-on").apply { isDaemon = true; start() }
        main.post(heartbeat)
        clockThread = Thread(::keepClock, "cratarr-clock").apply { isDaemon = true; start() }
        holdIfReady()
    }

    private var clockThread: Thread? = null

    /** Sync play (decision 0037): the clock's offset to Cratarr's, kept while running: quickly at first, then every 5 s.
     *  Unanswered (out of reach, or refused, as a proxy does off the home network), it waits longer each time, up to a
     *  minute, and starts quickly again once answered. */
    private fun keepClock() {
        var pinged = 0
        var failed = 0
        var server: String? = null
        while (running) {
            val now = prefs.serverUrl
            if (now != server) {
                Clock.forget()
                server = now
                pinged = 0
                failed = 0
            }
            if (now != null && Clock.ping(service, now)) {
                pinged++
                failed = 0
            } else {
                failed++
            }
            sleep(
                when {
                    failed > 0 -> (5_000L shl (failed - 1).coerceAtMost(4)).coerceAtMost(60_000L)
                    pinged < 10 -> 300
                    else -> 5_000
                },
            )
        }
    }

    fun stop() {
        closing = "stopped"
        running = false
        outputs?.stop()
        outputs = null
        clockThread?.interrupt()
        service.contentResolver.unregisterContentObserver(volumeSeen)
        player.removeListener(this)
        main.removeCallbacks(heartbeat)
        connection?.disconnect()
        thread?.interrupt()
        hold(false)
        reports.shutdown()
    }

    /** The address, the name or the session changed (`why`): connect again with them. */
    fun reconnect(why: String) {
        queueSent = false
        if (connection != null) closing = why
        connection?.disconnect()
        holdIfReady()
    }

    private fun holdIfReady() = hold(prefs.stayReady)

    // Held without a timeout on purpose: for as long as the device stays ready.
    @android.annotation.SuppressLint("WakelockTimeout")
    private fun hold(on: Boolean) {
        if (on) {
            if (!wakeLock.isHeld) wakeLock.acquire()
            if (!wifiLock.isHeld) wifiLock.acquire()
        } else {
            if (wakeLock.isHeld) wakeLock.release()
            if (wifiLock.isHeld) wifiLock.release()
        }
    }

    private fun base(): String? {
        val server = prefs.serverUrl?.trimEnd('/') ?: return null
        return "$server/api/players/${Uri.encode(prefs.playerName)}"
    }

    // --- the commands ---

    /**
     * The connection, kept open; after it drops, again after 2 s, then longer (at most a minute) while it fails. Each
     * connection has an id of its own (`X-Cratarr-Connection`), and what becomes of it goes into the link log: the
     * first failure of a run and each different one, how a connection ended and which side ended it (decision 0045).
     */
    private fun listen() {
        var wait = 2_000L
        while (running) {
            val base = base()
            if (base == null) {
                sleep(10_000)
                continue
            }
            val address = prefs.serverUrl
            val id = java.util.UUID.randomUUID().toString().take(12)
            Net.connection = id
            closing = null
            var opened = false
            var ended: String? = null
            try {
                val c = Net.open(service, "$base/connect")
                connection = c
                c.connectTimeout = 10_000
                // Cratarr sends a keep-alive every 15 s: a minute of silence is a dead connection.
                c.readTimeout = 60_000
                c.setRequestProperty("Accept", "text/event-stream")
                val code = Net.code(service, c)
                if (code == 409 && taken(c)) {
                    // Another account's player has this name (decision 0051): this one takes the next free one.
                    val next = nextName(prefs.playerName)
                    LinkLog.add(service, "warn", "playon", "name taken", "name" to prefs.playerName, "now" to next)
                    prefs.playerName = next
                    continue
                }
                if (code != 200) throw java.io.IOException("HTTP $code")
                opened = true
                LinkLog.add(service, "info", "playon", "connected", "address" to address, "conn" to id, "failed_before" to failed.takeIf { it > 0 })
                // Ready for what Cratarr sends: in the foreground (Stay ready), where Android lets the music start.
                if (prefs.stayReady) Ready.ensure(service, "connected")
                connectedSince = System.currentTimeMillis()
                failed = 0
                lastError = null
                waitUntil = 0L
                waitWhy = null
                wait = 2_000L
                // Back in reach: the plays made meanwhile (offline, say) are told now, and the link log's new lines.
                Plays.tell(service)
                if (!reports.isShutdown) reports.execute { LinkLog.send(service, base) }
                main.post { queueSent = false; report() }
                read(BufferedReader(InputStreamReader(c.inputStream, Charsets.UTF_8)))
                // The stream came to its end: Cratarr (or a proxy in between) closed it, unless the app did.
                ended = closing ?: "by Cratarr"
            } catch (e: java.net.SocketTimeoutException) {
                if (opened) ended = closing ?: "silence" else failedOnce(address, e)
            } catch (e: Exception) {
                if (opened) ended = closing ?: "network: ${e.message}" else if (running) failedOnce(address, e)
            } finally {
                connection?.disconnect()
                connection = null
            }
            if (opened) {
                connectedSince = 0L
                LinkLog.add(service, "info", "playon", "connection ended", "conn" to id, "by" to ended)
                main.post { report() }
            }
            if (!running) return
            // Out of reach there: perhaps the other address answers.
            if (!opened) Reach.checkSoon(service, failed = true)
            val held = address?.let { Net.heldUntil(service, it) }
            if (held != null && held > System.currentTimeMillis()) {
                // Refused there: its requests wait out the refusal (Net), so the connection does too.
                if (waitWhy != "refused") LinkLog.add(service, "warn", "playon", "waiting", "until" to held, "why" to "refused")
                waitUntil = held
                waitWhy = "refused"
                sleep((held - System.currentTimeMillis()).coerceIn(wait, Net.REFUSED_MS))
                continue
            }
            waitUntil = System.currentTimeMillis() + wait
            waitWhy = if (opened) "connecting again" else "after a failure"
            sleep(wait)
            wait = (wait * 2).coerceAtMost(60_000L)
        }
    }

    /** Whether a refused connection's answer says another account's player has the name (`{ error, taken: true }`). */
    private fun taken(c: java.net.HttpURLConnection): Boolean = try {
        c.errorStream?.use { JSONObject(it.readBytes().toString(Charsets.UTF_8)).optBoolean("taken") } ?: false
    } catch (_: Exception) {
        false
    }

    /** "Pixel 8" → "Pixel 8 2", "Pixel 8 2" → "Pixel 8 3": as Cratarr names a copy. */
    private fun nextName(name: String): String {
        val n = name.substringAfterLast(' ', "").toIntOrNull()
        return if (n != null && n in 2..98 && name.contains(' ')) "${name.substringBeforeLast(' ')} ${n + 1}" else "$name 2"
    }

    /** An attempt that failed: counted, and written down when it is the first of a run or fails otherwise than the last. */
    private fun failedOnce(address: String?, e: Exception) {
        val error = e.message ?: e.javaClass.simpleName
        failed++
        if (failed == 1 || error != lastError) {
            LinkLog.add(service, "warn", "playon", "connection failed", "address" to address, "error" to error, "attempt" to failed)
        }
        lastError = error
    }

    /** The link as this player sees it, in its reports, the times on Cratarr's clock (decision 0045). */
    private fun linkJson(): JSONObject {
        val offset = Clock.offsetMs.toLong()
        val out = JSONObject().put("failed", failed)
        if (connectedSince > 0) out.put("connected_since", connectedSince + offset)
        lastError?.let { out.put("last_error", it) }
        prefs.serverUrl?.let { out.put("address", it) }
        if (waitUntil > System.currentTimeMillis()) {
            out.put("wait_until", waitUntil + offset)
            waitWhy?.let { out.put("wait_why", it) }
        }
        return out
    }

    private fun read(reader: BufferedReader) {
        var event = ""
        val data = StringBuilder()
        while (running) {
            val line = reader.readLine() ?: return
            when {
                line.isEmpty() -> {
                    if (event == "command" && data.isNotEmpty()) {
                        val json = data.toString()
                        main.post { run(json) }
                    }
                    event = ""
                    data.setLength(0)
                }
                line.startsWith("event:") -> event = line.removePrefix("event:").trim()
                line.startsWith("data:") -> {
                    if (data.isNotEmpty()) data.append('\n')
                    data.append(line.removePrefix("data:").removePrefix(" "))
                }
            }
        }
    }

    private fun sleep(ms: Long) {
        try {
            Thread.sleep(ms)
        } catch (e: InterruptedException) {
            // stopped
        }
    }

    /** One command from Cratarr, on the player (main thread): the link's own here, the player's through [commands]. */
    private fun run(json: String) {
        val m = try {
            JSONObject(json)
        } catch (e: JSONException) {
            Log.w(TAG, "a command that cannot be read: $json")
            return
        }
        // Sync play's measurement (decision 0037, step 1): what plays this moment, in Cratarr's time, at once.
        if (m.optString("command") == "sample") return sample(m.optLong("id"))
        m.optLong("version", -1L).takeIf { it >= 0 }?.let { version = it }
        when (m.optString("command")) {
            // Sync play (decision 0037): the group this player plays in, by its leader (none: alone). Local mode
            // (decision 0052) plays alone.
            "group" -> {
                if (prefs.mode == Prefs.MODE_DEVICE && !m.isNull("leader")) return report()
                sync.group(
                    if (m.isNull("leader")) null else m.optString("leader"),
                    if (m.isNull("host")) null else m.optString("host").ifEmpty { null },
                    m.optInt("port", 1704),
                    m.optLong("buffer_ms", 1000),
                    m.optString("secret"),
                )
            }
            "volume" -> setVolume(m.optInt("level", 100))
            "duck" -> {
                ducked = true
                applyVolume()
            }
            "unduck" -> {
                ducked = false
                applyVolume()
            }
            // The announce channel (0.9): spoken over the music, which ducks by the speech itself (Announcer).
            "announce" -> Announcer.announce(
                service,
                m.optString("url"),
                m.optString("id"),
                musicPlaying = { player.isPlaying },
                heardLater = { if (sync.leads) sync.bufferMs else 0L },
                duck = { on ->
                    ducked = on
                    applyVolume()
                },
                told = { state ->
                    announcing = state
                    report()
                },
            )
            // A queue request asked of this account, or one of its own answered (decision 0079): a notification while
            // the app is not in front.
            "notice" -> return Notices.show(service, m.optString("text"))
            // The player's own, as the page's are (local mode's rules there too).
            else -> if (!commands.run(m, PlayerCommands.From.PLAY_ON)) Log.w(TAG, "a command this app does not know: $json")
        }
        report()
    }

    /** Sync play, step 1: this player's place this moment, in Cratarr's time, to Cratarr. */
    private fun sample(id: Long) {
        val p = player
        val position = p.currentPosition
        val at = Clock.now()
        val track = p.currentMediaItem?.let(::trackOf)
        val base = base() ?: return
        val body = JSONObject()
            .put("id", id)
            .put("entry", track?.optString("entry")?.ifEmpty { null } ?: JSONObject.NULL)
            .put("track", track?.let { "${it.optString("source")}/${it.optString("id")}" } ?: JSONObject.NULL)
            .put("playing", p.isPlaying)
            .put("position_ms", position.toDouble())
            .put("at_ms", at)
            .put("offset_ms", Clock.offsetMs)
            .put("rtt_ms", if (Clock.rttMs.isNaN()) 0.0 else Clock.rttMs)
            .apply {
                val (_, median, spread) = Clock.spread()
                if (!median.isNaN()) put("rtt_median_ms", median).put("offset_spread_ms", spread)
            }
            .toString()
        if (reports.isShutdown) return
        reports.execute { post("$base/sample", body) }
    }

    /** Shuffle, once (decision 0059): Cratarr shuffles what is to come in this player's queue and sends it back. */
    fun shuffleOnce() {
        val base = base() ?: return
        if (reports.isShutdown) return
        reports.execute { post("$base/command", JSONObject().put("command", "shuffle").put("on", true).toString()) }
    }

    private fun post(url: String, body: String): Boolean = try {
        val c = Net.open(service, url)
        c.requestMethod = "POST"
        c.connectTimeout = 5_000
        c.readTimeout = 5_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        val ok = Net.code(service, c) in 200..299
        c.disconnect()
        ok
    } catch (e: Exception) {
        Log.w(TAG, "sample to Cratarr: ${e.message}")
        false
    }

    // --- a start Android refuses ---

    /** Cratarr asked it to play, now; [again]: the same ask once more, after a refusal. */
    internal fun asked(again: Boolean = false) {
        askedAt = SystemClock.elapsedRealtime()
        if (!again) retried = false
    }

    override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
        // Refused: the player paused itself for want of the sound (Media3 says so for a refused request too), right
        // after Cratarr asked it to play. A loss later on is another app taking the sound, as ever.
        if (playWhenReady || reason != Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_FOCUS_LOSS) return
        if (SystemClock.elapsedRealtime() - askedAt > REFUSED_WITHIN_MS) return
        refused()
    }

    /**
     * Android would not let the music start. The first time: into the foreground (the Ready service; for a moment only
     * without Stay ready, as an update or a restart can leave a ready device out of it) and the play asked for again.
     * Again refused: held back, Cratarr told, and a notification to start it; it starts as the app comes to the front.
     */
    private fun refused() {
        val inFront = Ready.foreground(service)
        LinkLog.add(service, "warn", "playon", "play refused", "in_front" to inFront, "again" to !retried)
        if (retried) return hold()
        retried = true
        Ready.ensure(service, "play refused", briefly = !prefs.stayReady)
        val until = SystemClock.elapsedRealtime() + FRONT_WAIT_MS
        val retry = object : Runnable {
            override fun run() {
                if (player.mediaItemCount == 0 || player.playWhenReady) return
                // Asked again once it is in the foreground; not there in time (Android refused that too): asked anyway,
                // and a second refusal holds it.
                if (!Ready.foreground(service) && SystemClock.elapsedRealtime() < until) {
                    main.postDelayed(this, 250)
                    return
                }
                again = null
                commands.resume(PlayerCommands.From.PLAY_ON, again = true)
            }
        }
        again = retry
        main.postDelayed(retry, 250)
    }

    private fun hold() {
        heldBack = HELD_REFUSED
        heldAt = SystemClock.elapsedRealtime()
        askToPlay()
        soon()
    }

    /** Nothing held back any more: it plays, or it was paused or stopped, or another play came. */
    internal fun release() {
        again?.let(main::removeCallbacks)
        again = null
        if (heldBack == null) return
        heldBack = null
        service.getSystemService(NotificationManager::class.java)?.cancel(HELD_NOTIFICATION)
        soon()
    }

    /** The app came to the front, where Android lets it start music: what it held back plays now. */
    fun playHeld() {
        if (heldBack == null) return
        if (SystemClock.elapsedRealtime() - heldAt > HELD_MS || player.mediaItemCount == 0) return release()
        LinkLog.add(service, "info", "playon", "playing what was held back")
        commands.resume(PlayerCommands.From.PLAY_ON, again = true)
    }

    /** A notification to start the music held back: it opens the app, which starts it (above). */
    private fun askToPlay() {
        val notes = service.getSystemService(NotificationManager::class.java) ?: return
        notes.createNotificationChannel(NotificationChannel(HELD_CHANNEL, service.getString(R.string.tap_to_play), NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(
            service,
            0,
            Intent(service, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val note = Notification.Builder(service, HELD_CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(service.getString(R.string.tap_to_play))
            .setContentText(player.currentMediaItem?.mediaMetadata?.title ?: prefs.playerName)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        try {
            notes.notify(HELD_NOTIFICATION, note)
        } catch (e: SecurityException) {
            // Notifications not allowed: Cratarr is told all the same.
        }
    }

    // --- the reports ---

    /** The sleep timer changed (set here, by the page, or done): Cratarr is told. */
    fun sleepChanged() = soon()

    /** Local mode began or ended (decision 0052): Cratarr is told, Cratarr's queue again with the report after it. */
    fun modeChanged() {
        queueSent = false
        soon()
    }

    private val heartbeat = object : Runnable {
        override fun run() {
            // Ready, and found out of the foreground (Media3 let its service go, say): back in it.
            if (prefs.stayReady && !Ready.foreground(service)) Ready.ensure(service, "out of the foreground")
            // Held back too long ago: no longer what is wanted.
            if (heldBack != null && SystemClock.elapsedRealtime() - heldAt > HELD_MS) release()
            report()
            main.postDelayed(this, HEARTBEAT_MS)
        }
    }

    /** Ducked or not, levelled (decision 0035), faded (0.9.6, `StartStop`), under a track's snippet (0.12, `Snippets`):
     *  what the player plays at, under Android's media volume. */
    fun applyVolume() {
        player.volume = (if (ducked) DUCKED else 1f) * Levelling.factor * StartStop.gain * Snippets.duck
    }

    /** Android's media volume, 0 to 100. */
    private fun volume(): Int {
        val a = audio ?: return 100
        val max = a.getStreamMaxVolume(android.media.AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        return (a.getStreamVolume(android.media.AudioManager.STREAM_MUSIC) * 100 + max / 2) / max
    }

    /** Android's media volume set from 0 to 100 (its own steps, the nearest), as the keys would. */
    private fun setVolume(level: Int) {
        val a = audio ?: return
        val max = a.getStreamMaxVolume(android.media.AudioManager.STREAM_MUSIC)
        a.setStreamVolume(android.media.AudioManager.STREAM_MUSIC, (level.coerceIn(0, 100) * max + 50) / 100, 0)
        soon()
    }

    /** The levelling setting changed: the playing track levelled again (or not). */
    fun levellingChanged() = Levelling.follow(service, player, prefs.serverUrl, ::applyVolume, ramp = true)

    override fun onEvents(player: Player, events: Player.Events) {
        if (events.contains(Player.EVENT_TIMELINE_CHANGED)) queueSent = false
        if (events.contains(Player.EVENT_IS_PLAYING_CHANGED) && player.isPlaying) release()
        // Another track (or the queue around it): its levelling.
        if (events.containsAny(Player.EVENT_MEDIA_ITEM_TRANSITION, Player.EVENT_TIMELINE_CHANGED)) {
            Levelling.follow(service, player, prefs.serverUrl, ::applyVolume)
        }
        if (events.containsAny(
                Player.EVENT_MEDIA_ITEM_TRANSITION,
                Player.EVENT_PLAY_WHEN_READY_CHANGED,
                Player.EVENT_PLAYBACK_STATE_CHANGED,
                Player.EVENT_POSITION_DISCONTINUITY,
                Player.EVENT_TIMELINE_CHANGED,
                Player.EVENT_VOLUME_CHANGED,
            )
        ) {
            soon()
        }
    }

    /** Start and stop committed to a state, or let it go (0.9.6): Cratarr is told at once. */
    fun startStopChanged() = report()

    /** Changes close together go as one report. */
    private fun soon() {
        if (reportPending) return
        reportPending = true
        main.postDelayed({ reportPending = false; report() }, 300)
    }

    /** What plays now, to Cratarr (the queue too when it changed since the last report that reached it). */
    private fun report() {
        val base = base() ?: return
        val p = player
        if (prefs.mode == Prefs.MODE_DEVICE) return reportDevice(base)
        // The deck's one truth (decision 0075): a wind's place across the queue; otherwise the player's, where a group
        // hears it, or Start and stop's held place.
        val deck = service.startStop.deck()
        val index = if (p.mediaItemCount == 0) -1 else if (StartStop.winding) deck.index else p.currentMediaItemIndex
        val out = JSONObject()
            .put("index", index)
            .put("entry", (if (index >= 0) p.getMediaItemAt(index) else null)?.let(::entryOf) ?: JSONObject.NULL)
            .put("version", if (version >= 0) version else JSONObject.NULL)
            .put("repeat", Repeat.mode(service))
            .put("sleep", Sleep.json(service, "at_ms", "end_of_track"))
            .put("radio", Radio.on(service))
            .put("radio_by", Radio.by(service))
            .put("radio_count", Radio.count(service))
            .put("skip_favourites", SkipFavourites.on(service))
            .put("autoplay", Autoplay.json(service))
            // Leading a group, where it is heard: a buffer behind this player.
            .put("position_ms", (if (StartStop.winding) deck.ms else service.startStop.heldPosition ?: sync.heardPosition(p)).coerceAtLeast(0))
            .put("duration_ms", if (p.duration == C.TIME_UNSET) JSONObject.NULL else p.duration)
            // How it moves (0075): track ms per ms, signed, and its change per second; a page counts on from them.
            .put("speed_x", deck.speed)
            .put("accel_x", deck.accel)
            // A ramp under way says where it is going (0.9.6, StartStop): a pause is paused at once.
            .put("playing", service.startStop.committed ?: (p.playWhenReady && p.playbackState != Player.STATE_ENDED && p.playbackState != Player.STATE_IDLE))
            // Tape rewind (0.9.10): the position goes back to the start as the clip sounds; Cratarr does not count it on.
            .put("rewinding", service.startStop.rewinding)
            // A play from rest not sounding yet (0.9.14): the page's waveform grey until it does.
            .put("starting", StartStop.starting)
            .put("direction", StartStop.deckDirection)
            // Level the volume as set here (0.9.12): a page driving this player shows and sets it.
            .put("levelling", prefs.levelling)
            .put("level_unknown", prefs.levelUnknown)
            .put("level_db", Levelling.appliedDb)
            .put("volume", volume())
            .put("ducked", ducked)
            .put("announce", announcing?.json() ?: JSONObject.NULL)
            .put("held", heldBack ?: JSONObject.NULL)
            .put("output", Output.now(service) ?: JSONObject.NULL)
            .put("queue_id", Prefs(service).queueId.ifEmpty { null } ?: JSONObject.NULL)
            .put("queue_name", Prefs(service).queueName.ifEmpty { null } ?: JSONObject.NULL)
            .put("link", linkJson())
        val withQueue = !queueSent
        // Identities where Cratarr describes them (0.9.6); a server from before is told the details.
        if (withQueue) out.put("queue", queueOf(p, thin = Describe.available == true))
        val body = out.toString()
        if (reports.isShutdown) return
        reports.execute {
            val sent = put("$base/state", body)
            if (withQueue && sent) main.post { queueSent = true }
        }
    }

    /** In local mode (decision 0052): what plays of the device's own music, nothing of Cratarr's queue. */
    private fun reportDevice(base: String) {
        val p = player
        val track = p.currentMediaItem?.mediaMetadata?.let { d ->
            fun text(s: CharSequence?) = s?.toString()?.takeIf { it.isNotEmpty() } ?: JSONObject.NULL
            JSONObject().put("name", d.title?.toString().orEmpty()).put("artist", text(d.artist)).put("album", text(d.albumTitle))
        }
        val body = JSONObject()
            .put("mode", Prefs.MODE_DEVICE)
            .put("index", -1)
            .put("device_track", track ?: JSONObject.NULL)
            .put("position_ms", (service.startStop.heldPosition ?: p.currentPosition).coerceAtLeast(0))
            .put("duration_ms", if (p.duration == C.TIME_UNSET) JSONObject.NULL else p.duration)
            // A ramp under way says where it is going (0.9.6, StartStop): a pause is paused at once.
            .put("playing", service.startStop.committed ?: (p.playWhenReady && p.playbackState != Player.STATE_ENDED && p.playbackState != Player.STATE_IDLE))
            .put("volume", volume())
            .put("ducked", ducked)
            .put("announce", announcing?.json() ?: JSONObject.NULL)
            .put("held", heldBack ?: JSONObject.NULL)
            .put("output", Output.now(service) ?: JSONObject.NULL)
            .put("link", linkJson())
            .toString()
        if (reports.isShutdown) return
        reports.execute { put("$base/state", body) }
    }

    private fun put(url: String, body: String): Boolean {
        val (ok, error) = try {
            val c = Net.open(service, url)
            c.requestMethod = "PUT"
            c.connectTimeout = 10_000
            c.readTimeout = 10_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            val code = Net.code(service, c)
            c.disconnect()
            (code in 200..299) to "HTTP $code"
        } catch (e: Exception) {
            Log.w(TAG, "report to Cratarr: ${e.message}")
            false to (e.message ?: e.javaClass.simpleName)
        }
        // Written down as it changes, not every report.
        if (ok != reportsThrough) {
            reportsThrough = ok
            if (ok) LinkLog.add(service, "info", "playon", "reports through") else LinkLog.add(service, "warn", "playon", "reports failing", "error" to error)
        }
        return ok
    }

    private companion object {
        const val TAG = "CratarrPlayOn"
        const val HEARTBEAT_MS = 30_000L
        /** A ducked player plays at this part of its volume. */
        const val DUCKED = 0.25f
        /** A pause for want of the sound this soon after Cratarr asked it to play is Android refusing the start. */
        const val REFUSED_WITHIN_MS = 3_000L
        /** How long a refused start waits for the foreground before it is asked again all the same. */
        const val FRONT_WAIT_MS = 3_000L
        /** Music held back still starts as the app comes to the front this long after; later it is let go. */
        const val HELD_MS = 10 * 60_000L
        /** What Cratarr is told while a start is held back (its `held`). */
        const val HELD_REFUSED = "Android would not let it play"
        const val HELD_CHANNEL = "held"
        const val HELD_NOTIFICATION = 10
    }
}
