package app.cratarr

import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.Player
import org.json.JSONArray
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

    /** The volume as set (0 to 1), ducked or not; the player plays it times the duck, times the track's levelling. */
    private var volume = 1f
    private var ducked = false
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

    fun start() {
        if (running) return
        running = true
        volume = player.volume
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
        clockThread?.interrupt()
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
                if (code != 200) throw java.io.IOException("HTTP $code")
                opened = true
                LinkLog.add(service, "info", "playon", "connected", "address" to address, "conn" to id, "failed_before" to failed.takeIf { it > 0 })
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

    /** One command from Cratarr, on the player (main thread). */
    private fun run(json: String) {
        val m = try {
            JSONObject(json)
        } catch (e: JSONException) {
            Log.w(TAG, "a command that cannot be read: $json")
            return
        }
        val p = player
        // Sync play's measurement (decision 0037, step 1): what plays this moment, in Cratarr's time, at once.
        if (m.optString("command") == "sample") return sample(m.optLong("id"))
        m.optLong("version", -1L).takeIf { it >= 0 }?.let { version = it }
        when (m.optString("command")) {
            "play" -> {
                val items = mediaItems(m.optJSONArray("tracks") ?: JSONArray()).map(service::located)
                if (items.isEmpty()) return
                p.setMediaItems(items, m.optInt("index", 0).coerceIn(0, items.size - 1), m.optLong("position_ms", 0))
                p.prepare()
                // A new play turns repeat one off; whether it comes shuffled counts for repeat all's next rounds; the radio
                // stays on, its seed the new queue (unless these are its own tracks).
                Repeat.newPlay(service, p)
                Autoplay.newPlay(service, m.optJSONArray("tracks"))
                Prefs(service).shuffled = m.optBoolean("shuffle")
                // A queue given back after the app came back without one waits for a play.
                if (m.optBoolean("paused")) p.pause() else p.play()
            }
            // Edits name entries (decision 0033); by place only when the entry is not found here.
            "add" -> {
                val items = mediaItems(m.optJSONArray("tracks") ?: JSONArray()).map(service::located)
                val after = m.optString("after", null)?.let { if (it.isEmpty()) 0 else placeOf(p, it).takeIf { a -> a >= 0 }?.plus(1) }
                val at = after ?: if (m.has("at")) m.optInt("at").coerceIn(0, p.mediaItemCount) else p.mediaItemCount
                p.addMediaItems(at, items)
                if (p.playbackState == Player.STATE_IDLE) p.prepare()
            }
            "move" -> {
                val from = placed(m, "entry", "from")
                val after = m.optString("after", null)?.let { if (it.isEmpty()) 0 else placeOf(p, it) }
                // The place it ends up at, counted with it taken out.
                val to = when {
                    after == null || after < 0 -> m.optInt("to", -1)
                    after == 0 && m.optString("after").isEmpty() -> 0
                    after > from -> after
                    else -> after + 1
                }
                if (from in 0 until p.mediaItemCount && to in 0 until p.mediaItemCount && from != to) p.moveMediaItem(from, to)
            }
            "remove" -> placed(m, "entry", "index").takeIf { it in 0 until p.mediaItemCount }?.let(p::removeMediaItem)
            // Everything after the playing track replaced (a shuffle turned on or off); the playing track plays on.
            "upcoming" -> {
                if (m.has("shuffled")) Prefs(service).shuffled = m.optBoolean("shuffled")
                val items = mediaItems(m.optJSONArray("tracks") ?: JSONArray()).map(service::located)
                val from = (p.currentMediaItemIndex + 1).coerceAtMost(p.mediaItemCount)
                p.replaceMediaItems(from, p.mediaItemCount, items)
            }
            "jump" -> placed(m, "entry", "index").takeIf { it in 0 until p.mediaItemCount }?.let {
                p.seekTo(it, 0)
                play()
            }
            "pause" -> p.pause()
            "resume" -> play()
            "toggle" -> if (p.playWhenReady && p.playbackState != Player.STATE_ENDED) p.pause() else play()
            "stop" -> {
                p.stop()
                p.clearMediaItems()
            }
            "next" -> p.seekToNextMediaItem()
            "previous" -> if (!Repeat.previous(service, p)) p.seekToPrevious()
            "repeat" -> Repeat.set(service, p, m.optString("mode"), service::located)
            "sleep" -> Sleep.set(service, m, "at_ms", "end_of_track")
            // Sync play (decision 0037): the group this player plays in, by its leader (none: alone).
            "group" -> sync.group(
                if (m.isNull("leader")) null else m.optString("leader"),
                if (m.isNull("host")) null else m.optString("host").ifEmpty { null },
                m.optInt("port", 1704),
                m.optLong("buffer_ms", 1000),
            )
            "radio" -> Radio.command(service, p, m)
            "autoplay" -> Autoplay.command(service, p, m)
            "skip_favourites" -> SkipFavourites.set(service, p, m.optBoolean("on"))
            "seek" -> p.seekTo(m.optLong("position_ms", p.currentPosition))
            "volume" -> {
                volume = m.optInt("level", 100).coerceIn(0, 100) / 100f
                applyVolume()
            }
            "duck" -> {
                ducked = true
                applyVolume()
            }
            "unduck" -> {
                ducked = false
                applyVolume()
            }
            else -> Log.w(TAG, "a command this app does not know: $json")
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

    /** The place of the entry a command names, else the place it gives. */
    private fun placed(m: JSONObject, entry: String, place: String): Int =
        m.optString(entry, null)?.let { placeOf(player, it) }?.takeIf { it >= 0 } ?: m.optInt(place, -1)

    private fun play() {
        val p = player
        if (p.playbackState == Player.STATE_IDLE) p.prepare()
        if (p.playbackState == Player.STATE_ENDED) p.seekTo(p.currentMediaItemIndex, 0)
        p.play()
    }

    // --- the reports ---

    /** The sleep timer changed (set here, by the page, or done): Cratarr is told. */
    fun sleepChanged() = soon()

    private val heartbeat = object : Runnable {
        override fun run() {
            report()
            main.postDelayed(this, HEARTBEAT_MS)
        }
    }

    /** The volume as set, ducked, levelled (decision 0035): what the player plays at. */
    private fun applyVolume() {
        player.volume = volume * (if (ducked) DUCKED else 1f) * Levelling.factor
    }

    /** The levelling setting changed: the playing track levelled again (or not). */
    fun levellingChanged() = Levelling.follow(service, player, prefs.serverUrl, ::applyVolume)

    override fun onEvents(player: Player, events: Player.Events) {
        if (events.contains(Player.EVENT_TIMELINE_CHANGED)) queueSent = false
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
        val out = JSONObject()
            .put("index", if (p.mediaItemCount == 0) -1 else p.currentMediaItemIndex)
            .put("entry", p.currentMediaItem?.let(::entryOf) ?: JSONObject.NULL)
            .put("version", if (version >= 0) version else JSONObject.NULL)
            .put("repeat", Repeat.mode(service))
            .put("sleep", Sleep.json(service, "at_ms", "end_of_track"))
            .put("radio", Radio.on(service))
            .put("radio_by", Radio.by(service))
            .put("radio_count", Radio.count(service))
            .put("skip_favourites", SkipFavourites.on(service))
            .put("autoplay", Autoplay.json(service))
            // Leading a group, where it is heard: a buffer behind this player.
            .put("position_ms", sync.heardPosition(p).coerceAtLeast(0))
            .put("duration_ms", if (p.duration == C.TIME_UNSET) JSONObject.NULL else p.duration)
            .put("playing", p.playWhenReady && p.playbackState != Player.STATE_ENDED && p.playbackState != Player.STATE_IDLE)
            .put("volume", (volume * 100).toInt())
            .put("ducked", ducked)
            .put("link", linkJson())
        val withQueue = !queueSent
        if (withQueue) out.put("queue", queueOf(p))
        val body = out.toString()
        if (reports.isShutdown) return
        reports.execute {
            val sent = put("$base/state", body)
            if (withQueue && sent) main.post { queueSent = true }
        }
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
    }
}
