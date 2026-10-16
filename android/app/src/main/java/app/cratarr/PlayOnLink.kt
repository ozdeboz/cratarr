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
import java.net.URL
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

    /** The volume before a duck (0 to 1); null when not ducked. */
    private var unducked: Float? = null
    private var queueSent = false
    /** The queue's version in the last command from Cratarr: reports say it, so Cratarr knows what they have seen. */
    private var version = -1L
    private var reportPending = false

    fun start() {
        if (running) return
        running = true
        player.addListener(this)
        thread = Thread(::listen, "cratarr-play-on").apply { isDaemon = true; start() }
        main.post(heartbeat)
        clockThread = Thread(::keepClock, "cratarr-clock").apply { isDaemon = true; start() }
        holdIfReady()
    }

    private var clockThread: Thread? = null

    /** Sync play (decision 0037): the clock's offset to Cratarr's, kept while running: quickly at first, then every 5 s. */
    private fun keepClock() {
        var pinged = 0
        var server: String? = null
        while (running) {
            val now = prefs.serverUrl
            if (now != server) {
                Clock.forget()
                server = now
                pinged = 0
            }
            if (now != null && Clock.ping(now)) pinged++
            sleep(if (pinged < 10) 300 else 5_000)
        }
    }

    fun stop() {
        running = false
        clockThread?.interrupt()
        player.removeListener(this)
        main.removeCallbacks(heartbeat)
        connection?.disconnect()
        thread?.interrupt()
        hold(false)
        reports.shutdown()
    }

    /** The address or the name changed: connect again with them. */
    fun reconnect() {
        queueSent = false
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

    /** The connection, kept open; after it drops, again after 2 s, then longer (at most a minute) while it fails. */
    private fun listen() {
        var wait = 2_000L
        while (running) {
            val base = base()
            if (base == null) {
                sleep(10_000)
                continue
            }
            try {
                val c = URL("$base/connect").openConnection() as HttpURLConnection
                connection = c
                c.connectTimeout = 10_000
                // Cratarr sends a keep-alive every 15 s: a minute of silence is a dead connection.
                c.readTimeout = 60_000
                c.setRequestProperty("Accept", "text/event-stream")
                if (c.responseCode != 200) throw java.io.IOException("HTTP ${c.responseCode}")
                Log.i(TAG, "connected as ${prefs.playerName}")
                wait = 2_000L
                // Back in reach: the plays made meanwhile (offline, say) are told now.
                Plays.tell(service)
                main.post { queueSent = false; report() }
                read(BufferedReader(InputStreamReader(c.inputStream, Charsets.UTF_8)))
            } catch (e: Exception) {
                if (running) Log.w(TAG, "connection to Cratarr: ${e.message}")
            } finally {
                connection?.disconnect()
                connection = null
            }
            if (!running) return
            sleep(wait)
            wait = (wait * 2).coerceAtMost(60_000L)
        }
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
            "radio" -> Radio.command(service, p, m)
            "autoplay" -> Autoplay.command(service, p, m)
            "seek" -> p.seekTo(m.optLong("position_ms", p.currentPosition))
            "volume" -> {
                val v = m.optInt("level", 100).coerceIn(0, 100) / 100f
                if (unducked != null) unducked = v else p.volume = v
            }
            "duck" -> if (unducked == null) {
                unducked = p.volume
                p.volume = p.volume * DUCKED
            }
            "unduck" -> unducked?.let {
                p.volume = it
                unducked = null
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
            .toString()
        if (reports.isShutdown) return
        reports.execute { post("$base/sample", body) }
    }

    private fun post(url: String, body: String): Boolean = try {
        val c = URL(url).openConnection() as HttpURLConnection
        c.requestMethod = "POST"
        c.connectTimeout = 5_000
        c.readTimeout = 5_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        val ok = c.responseCode in 200..299
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

    override fun onEvents(player: Player, events: Player.Events) {
        if (events.contains(Player.EVENT_TIMELINE_CHANGED)) queueSent = false
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
            .put("autoplay", Autoplay.json(service))
            .put("position_ms", p.currentPosition.coerceAtLeast(0))
            .put("duration_ms", if (p.duration == C.TIME_UNSET) JSONObject.NULL else p.duration)
            .put("playing", p.playWhenReady && p.playbackState != Player.STATE_ENDED && p.playbackState != Player.STATE_IDLE)
            .put("volume", ((unducked ?: p.volume) * 100).toInt())
            .put("ducked", unducked != null)
        val withQueue = !queueSent
        if (withQueue) out.put("queue", queueOf(p))
        val body = out.toString()
        if (reports.isShutdown) return
        reports.execute {
            val sent = put("$base/state", body)
            if (withQueue && sent) main.post { queueSent = true }
        }
    }

    private fun put(url: String, body: String): Boolean = try {
        val c = URL(url).openConnection() as HttpURLConnection
        c.requestMethod = "PUT"
        c.connectTimeout = 10_000
        c.readTimeout = 10_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        val ok = c.responseCode in 200..299
        c.disconnect()
        ok
    } catch (e: Exception) {
        Log.w(TAG, "report to Cratarr: ${e.message}")
        false
    }

    private companion object {
        const val TAG = "CratarrPlayOn"
        const val HEARTBEAT_MS = 30_000L
        /** A ducked player plays at this part of its volume. */
        const val DUCKED = 0.25f
    }
}
