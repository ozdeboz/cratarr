package app.cratarr

import android.content.Context
import android.content.SharedPreferences
import android.os.Bundle
import android.os.SystemClock
import androidx.core.content.ContextCompat

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaController
import androidx.media3.session.SessionCommand
import org.json.JSONArray
import org.json.JSONObject

/**
 * Between the page's local player and the app's own (decision 0031, amended). The page's calls (`{"type": "player",
 * "op": …}`) become the player's commands, in Cratarr's own shape ([PageCommand]), run in the service as Play On's are
 * ([PlayerService.COMMAND_RUN], [PlayerCommands]); what the player does goes back to the page as `{"type": "player",
 * "index", "positionMs", "durationMs", "paused", "error", "queue"?}`. The queue's tracks are the page's own (source, id,
 * name, artist, album, album_id), kept with each media item, so a queue the player changed itself (Play On) reaches the
 * page as it is. Local mode (decision 0052) plays through the same calls, with the device's own tracks (`source`
 * device); its reports say which mode's queue they are (`mode`).
 */
@OptIn(UnstableApi::class)
class PagePlayer(private val context: Context, private val controller: MediaController, private val send: (String) -> Unit) : Player.Listener {
    /** The page asks where this device's controls point (decision 0062); the service decides and the page hears it. */
    fun pointAt(to: String) {
        controller.sendCustomCommand(SessionCommand(PlayerService.COMMAND_TARGET, Bundle.EMPTY), Bundle().apply { putString("to", to) })
    }

    /** Hold-to-preview (the queue's covers): a second player for the held track, while the music waits. */
    private var previewer: ExoPlayer? = null
    /** The music played when the preview began: it goes on after. */
    private var resumeAfter = false
    /** A mode switch under way (decision 0052): the page's calls meanwhile wait for it. */
    private var switching: MutableList<JSONObject>? = null

    /** The sleep timer changed (set, or done): the page is told. Held here, as Android keeps listeners only weakly. */
    private val sleepChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == Prefs.SLEEP || key == Prefs.RADIO_ON || key == Prefs.RADIO_BY || key == Prefs.RADIO_COUNT || key == Prefs.AUTOPLAY || key == Prefs.SKIP_FAVOURITES) report()
    }

    init {
        controller.addListener(this)
        Prefs.shared(context).registerOnSharedPreferenceChangeListener(sleepChanged)
    }

    fun release() {
        StartStop.rampListener = null
        previewer?.release()
        previewer = null
        controller.removeListener(this)
        Prefs.shared(context).unregisterOnSharedPreferenceChangeListener(sleepChanged)
    }

    /** A command (Cratarr's shape) to the service, which runs it as the page's ([PlayerCommands]). */
    private fun run(command: JSONObject) {
        controller.sendCustomCommand(SessionCommand(PlayerService.COMMAND_RUN, Bundle.EMPTY), Bundle().apply { putString("command", command.toString()) })
    }

    init {
        // A ramp's direction to the page (0.9.6): its play button draws it.
        // A wind-up shortened near a track's start (0.9.7) says so: `ms` its length, `full` the length set.
        // `effect`: whether it ends with the music stopped (a skip's wind-down does not: no grey, no drain).
        // `from` (0.9.15): the level it starts at, 0 still to 1 playing; a ramp turned round (a press the other way
        // during it, 0075) starts where it was turned, over `ms` = `full` × the share left.
        StartStop.rampListener = { dir, ms, full, stops, from ->
            val m = JSONObject().put("type", "ramp").put("dir", dir ?: JSONObject.NULL).put("ms", ms).put("full", full)
                .put("from", from.toDouble())
            if (dir != null) m.put("effect", stops)
            send(m.toString())
        }
        // The deck as it moves (0075): told each step of a wind or the tape's own ramp, and once more as it stops.
        StartStop.deckListener = { report() }
    }

    fun handle(m: JSONObject) {
        // Where the controls point, asked before the player was connected (MainActivity's waiting messages).
        if (m.optString("type") == "target") return pointAt(m.optString("to"))
        switching?.let {
            it.add(m)
            return
        }
        // The page's own: local mode (decision 0052) on or off, from the app's switch (Android's permission given), and
        // hold-to-preview.
        when (m.optString("op")) {
            "mode" -> return switchMode(m.optString("mode"), null)
            "preview" -> return preview(m.optJSONObject("track"), m.optLong("positionMs"))
            "preview-end" -> return previewEnd()
        }
        val command = PageCommand.of(m) ?: return
        if (command.optString("command") == "play") {
            // A play of the other mode's music is that mode again (Android's permission given), never both in one.
            val mode = Prefs(context).mode
            val want = modeOf(command.optJSONArray("tracks")) ?: mode
            if (want != mode) {
                if (want == Prefs.MODE_DEVICE && !DeviceMusic.on(context)) return
                if (!m.optBoolean("switched")) switchMode(want, JSONObject(m.toString()).put("switched", true))
                return
            }
        }
        run(command)
    }

    /**
     * Local mode on or off (decision 0052): the service puts one queue by and takes up the other; the page's calls
     * meanwhile, and [then] (the play that asked for it), wait for it, and the page hears the new mode's queue.
     */
    private fun switchMode(to: String, then: JSONObject?) {
        val waiting = mutableListOf<JSONObject>().also { switching = it }
        then?.let(waiting::add)
        val args = Bundle().apply { putString("mode", to) }
        controller.sendCustomCommand(SessionCommand(PlayerService.COMMAND_MODE, Bundle.EMPTY), args).addListener(
            {
                switching = null
                report(withQueue = true)
                waiting.forEach(::handle)
            },
            ContextCompat.getMainExecutor(context),
        )
    }

    /** The held track from `positionMs`, the music paused meanwhile. */
    private fun preview(track: JSONObject?, positionMs: Long) {
        val item = track?.let { mediaItems(JSONArray().put(it)).firstOrNull() } ?: return
        val located = DeviceMusic.located(item) ?: Downloads.locate(context, item, Prefs(context).serverUrl?.trimEnd('/'))
        val p = previewer ?: ExoPlayer.Builder(context)
            .setMediaSourceFactory(DefaultMediaSourceFactory(androidx.media3.datasource.DefaultDataSource.Factory(context, Downloads.playback(context))))
            .build()
            .also { previewer = it }
        if (!p.isPlaying) resumeAfter = controller.playWhenReady && controller.playbackState != Player.STATE_ENDED
        controller.pause()
        p.setMediaItem(located, positionMs.coerceAtLeast(0))
        p.prepare()
        p.play()
    }

    /** The finger lifted: the music goes on where it was. */
    private fun previewEnd() {
        previewer?.stop()
        if (resumeAfter) run(JSONObject().put("command", "resume"))
        resumeAfter = false
    }

    /** What plays now; with the queue on request (the page said hello) and whenever it changed. */
    fun report(withQueue: Boolean = false) {
        val c = controller
        // The deck's one truth (decision 0075): where it is, across the queue, and how it moves; the page counts on
        // from it each frame (place + speedX·t + ½·accelX·t², t in seconds). Without the service: the controller's.
        // When the deck was read (elapsed realtime): the page counts on from then, not from when the message reached it.
        val sampled = SystemClock.elapsedRealtime()
        val deck = StartStop.deckNow?.invoke()?.takeIf { c.mediaItemCount > 0 }
        val index = deck?.index ?: if (c.mediaItemCount == 0) -1 else c.currentMediaItemIndex
        val length = lengthOf(c, index)
        val out = JSONObject()
            .put("type", "player")
            .put("mode", Prefs(context).mode)
            .put("index", index)
            .put("positionMs", deck?.ms ?: c.currentPosition)
            .put("speedX", deck?.speed ?: if (c.isPlaying) c.playbackParameters.speed.toDouble() else 0.0)
            .put("accelX", deck?.accel ?: 0.0)
            .put("deckAtMs", sampled)
            // The place is a wind's (its stream sounds).
            .put("rewinding", StartStop.winding)
            // A hold's aim (the page's look-ahead): forwards where it plays at 1× again, backwards where it stands;
            // held, where letting go now aims; let go, the aim it was let go with.
            .put("landing", StartStop.landingNow?.invoke()?.let { JSONObject().put("index", it.index).put("positionMs", it.ms) } ?: JSONObject.NULL)
            .put("direction", StartStop.deckDirection)
            .put("durationMs", length)
            .put("paused", !c.playWhenReady || c.playbackState == Player.STATE_ENDED)
            // A play from rest not sounding yet (0.9.14, as Play On's `starting`).
            .put("starting", StartStop.starting)
            .put("error", c.playerError?.let(::describe) ?: JSONObject.NULL)
            .put("repeat", Repeat.mode(context))
            .put("sleep", Sleep.json(context, "atMs", "endOfTrack"))
            .put("radio", Radio.on(context))
            .put("radioBy", Radio.by(context))
            .put("radioCount", Radio.count(context))
            .put("skipFavourites", SkipFavourites.on(context))
            .put("autoplay", Autoplay.json(context))
            // What levelling turns the playing track down by (0.9.8, the Level button): 0 when off.
            .put("levelDb", Levelling.appliedDb)
        if (withQueue) out.put("queue", queueOf(c))
        send(out.toString())
    }

    /** How long the queue's track [index] is (ms), as its file is; -1 unknown. */
    private fun lengthOf(c: Player, index: Int): Long {
        if (index == c.currentMediaItemIndex) return if (c.duration == C.TIME_UNSET) -1 else c.duration
        val tl = c.currentTimeline
        if (index < 0 || index >= tl.windowCount) return -1
        return tl.getWindow(index, Timeline.Window()).durationMs.takeIf { it != C.TIME_UNSET } ?: -1
    }

    override fun onEvents(player: Player, events: Player.Events) {
        if (events.containsAny(
                Player.EVENT_MEDIA_ITEM_TRANSITION,
                Player.EVENT_PLAY_WHEN_READY_CHANGED,
                Player.EVENT_IS_PLAYING_CHANGED,
                Player.EVENT_PLAYBACK_STATE_CHANGED,
                Player.EVENT_POSITION_DISCONTINUITY,
                Player.EVENT_TIMELINE_CHANGED,
                Player.EVENT_PLAYER_ERROR,
            )
        ) {
            report(withQueue = events.contains(Player.EVENT_TIMELINE_CHANGED))
        }
    }

    /** The mode tracks are of (the first's), null for none. */
    private fun modeOf(tracks: JSONArray?): String? {
        val t = (0 until (tracks?.length() ?: 0)).firstNotNullOfOrNull { tracks?.optJSONObject(it) } ?: return null
        return if (t.optString("source") == DeviceMusic.SOURCE) Prefs.MODE_DEVICE else Prefs.MODE_CRATARR
    }

    private fun describe(e: PlaybackException): String {
        val name = controller.currentMediaItem?.mediaMetadata?.title ?: "A track"
        return "$name: it could not be played (${e.errorCodeName})."
    }
}

/**
 * The page's player calls (`{"type": "player", "op": …}`, camelCase) as the commands Cratarr sends a player (its
 * `Command`, snake_case), which the app's player runs one way ([PlayerCommands]): `queue` is `play` (its `positionMs`,
 * `play` and `shuffled` as `position_ms`, `paused` and `shuffle`, its `name` kept), `play` is `resume`, and `seek`'s
 * `positionMs` and `sleep`'s `atMs` and `endOfTrack` are snake_case; the rest go by their own name and fields. A message
 * that is a command already (`command`) goes as it is. Null for the page's own (`mode`, `preview`, `preview-end`) and
 * anything else.
 */
object PageCommand {
    /** The calls that are commands of the same name, with the same fields. */
    private val SAME = setOf(
        "add", "move", "remove", "upcoming", "jump", "next", "previous", "pause", "toggle", "stop",
        "repeat", "radio", "autoplay", "skip_favourites", "direction", "wind",
    )

    fun of(m: JSONObject): JSONObject? {
        val out = JSONObject(m.toString())
        out.remove("type")
        // The mark of a play that waited for a mode switch (PagePlayer), not the command's.
        out.remove("switched")
        if (m.has("command")) return out
        val op = m.optString("op")
        out.remove("op")
        val command = when (op) {
            in SAME -> op
            "queue" -> {
                out.remove("play")
                out.put("paused", !m.optBoolean("play", true))
                rename(out, "positionMs", "position_ms")
                rename(out, "shuffled", "shuffle")
                "play"
            }
            "play" -> "resume"
            "seek" -> {
                rename(out, "positionMs", "position_ms")
                "seek"
            }
            "sleep" -> {
                rename(out, "atMs", "at_ms")
                rename(out, "endOfTrack", "end_of_track")
                "sleep"
            }
            else -> return null
        }
        return out.put("command", command)
    }

    /** Field [from] as [to], its value as it is (a null too). */
    private fun rename(o: JSONObject, from: String, to: String) {
        if (!o.has(from)) return
        o.put(to, o.get(from))
        o.remove(from)
    }
}
