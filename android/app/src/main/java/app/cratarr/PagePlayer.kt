package app.cratarr

import android.content.Context
import android.content.SharedPreferences
import android.os.Bundle
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
 * "op": …}`) become the player's; what the player does goes back to the page as `{"type": "player", "index",
 * "positionMs", "durationMs", "paused", "error", "queue"?}`. The queue's tracks are the page's own (source, id, name,
 * artist, album, album_id), kept with each media item, so a queue the player changed itself (Play On) reaches the page
 * as it is. Local mode (decision 0052) plays through the same calls, with the device's own tracks (`source` device);
 * its reports say which mode's queue they are (`mode`).
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

    private fun skip(dir: String) {
        controller.sendCustomCommand(SessionCommand(PlayerService.COMMAND_SKIP, Bundle.EMPTY), Bundle().apply { putString("dir", dir) })
    }

    init {
        // A ramp's direction to the page (0.9.6): its play button draws it.
        // A wind-up shortened near a track's start (0.9.7) says so: `ms` its length, `full` the length set.
        // `effect`: whether it ends with the music stopped (a skip's wind-down does not: no grey, no drain).
        StartStop.rampListener = { dir, ms, full, stops ->
            val m = JSONObject().put("type", "ramp").put("dir", dir ?: JSONObject.NULL).put("ms", ms).put("full", full)
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
        val c = controller
        val mode = Prefs(context).mode
        val local = mode == Prefs.MODE_DEVICE
        when (m.optString("op")) {
            // Local mode (decision 0052) on or off, from the app's switch (Android's permission given).
            "mode" -> switchMode(m.optString("mode"), null)
            "queue" -> {
                // A play of the other mode's music is that mode again (Android's permission given), never both in one.
                val want = modeOf(m.optJSONArray("tracks")) ?: mode
                if (want != mode) {
                    if (want == Prefs.MODE_DEVICE && !DeviceMusic.on(context)) return
                    if (!m.optBoolean("switched")) switchMode(want, JSONObject(m.toString()).put("switched", true))
                    return
                }
                // A new play turns repeat one off; whether it comes shuffled counts for repeat all's next rounds.
                Repeat.newPlay(context, c)
                Prefs(context).shuffled = m.optBoolean("shuffled")
                // Another queue, started here (decision 0033, amended): its own id and the play's name; Cratarr keeps the
                // one before. Local mode's is its own.
                if (!local) {
                    Autoplay.newPlay(context, m.optJSONArray("tracks"))
                    Prefs(context).newQueue(m.optString("name"))
                }
                c.setMediaItems(mediaItems(only(m.optJSONArray("tracks"), mode)), m.optInt("index", 0), m.optLong("positionMs", 0))
                c.prepare()
                c.playWhenReady = m.optBoolean("play", true)
            }
            "add" -> c.addMediaItems(m.optInt("at", c.mediaItemCount), mediaItems(only(m.optJSONArray("tracks"), mode)))
            "move" -> c.moveMediaItem(m.optInt("from"), m.optInt("to"))
            "remove" -> c.removeMediaItem(m.optInt("index"))
            // Everything after the playing track replaced (the shuffle switch); the playing track plays on.
            "upcoming" -> {
                if (m.has("shuffled")) Prefs(context).shuffled = m.optBoolean("shuffled")
                val from = (c.currentMediaItemIndex + 1).coerceAtMost(c.mediaItemCount)
                c.replaceMediaItems(from, c.mediaItemCount, mediaItems(only(m.optJSONArray("tracks"), mode)))
            }
            "jump" -> {
                c.seekTo(m.optInt("index"), 0)
                play()
            }
            // Through the service (0.9.6, Start and stop): the change shows once it is made, after a ramp out.
            "next" -> skip("next")
            "previous" -> skip("previous")
            // The tape deck's direction knob (0.9.12), through the service.
            "direction" -> controller.sendCustomCommand(
                SessionCommand(PlayerService.COMMAND_DIRECTION, Bundle.EMPTY),
                Bundle().apply { putString("dir", m.optString("dir")) },
            )
            // Previous or Next held (decision 0074): wound while held, through the service as Play On's `wind`.
            "wind" -> controller.sendCustomCommand(
                SessionCommand(PlayerService.COMMAND_WIND, Bundle.EMPTY),
                Bundle().apply {
                    putString("dir", m.optString("dir"))
                    putString("phase", m.optString("phase"))
                },
            )
            // Its items are located by the service when they reach it.
            "repeat" -> Repeat.set(context, c, m.optString("mode")) { it }
            // Kept by the service (Sleep); the change reaches the page as a report.
            "sleep" -> Sleep.set(context, m, "atMs", "endOfTrack")
            // Kept and filled by the service (Radio); the change reaches the page as a report.
            // Cratarr's alone: not for the device's own music.
            "radio" -> if (!local) Radio.command(context, c, m)
            "autoplay" -> if (!local) Autoplay.command(context, c, m)
            "skip_favourites" -> if (!local) SkipFavourites.set(context, c, m.optBoolean("on"))
            // `wind`: the page's own seek winding (0.9.12), wound there whatever this device says.
            "seek" -> if (m.optBoolean("wind")) {
                controller.sendCustomCommand(
                    SessionCommand(PlayerService.COMMAND_SEEK_WIND, Bundle.EMPTY),
                    Bundle().apply { putLong("positionMs", m.optLong("positionMs")) },
                )
            } else {
                c.seekTo(m.optLong("positionMs"))
            }
            "preview" -> preview(m.optJSONObject("track"), m.optLong("positionMs"))
            "preview-end" -> previewEnd()
            "play" -> play()
            "pause" -> c.pause()
            "toggle" -> if (c.playWhenReady && c.playbackState != Player.STATE_ENDED) c.pause() else play()
            "stop" -> {
                c.stop()
                c.clearMediaItems()
            }
        }
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
        if (resumeAfter) play()
        resumeAfter = false
    }

    private fun play() {
        val c = controller
        if (c.playbackState == Player.STATE_IDLE) c.prepare()
        if (c.playbackState == Player.STATE_ENDED) c.seekTo(c.currentMediaItemIndex, 0)
        c.play()
    }

    /** What plays now; with the queue on request (the page said hello) and whenever it changed. */
    fun report(withQueue: Boolean = false) {
        val c = controller
        // The deck's one truth (decision 0075): where it is, across the queue, and how it moves; the page counts on
        // from it each frame (place + speedX·t + ½·accelX·t², t in seconds). Without the service: the controller's.
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
            // The place is a wind's (its stream sounds).
            .put("rewinding", StartStop.winding)
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

    /** Only the tracks of [mode]: never both in one queue. */
    private fun only(tracks: JSONArray?, mode: String): JSONArray {
        val out = JSONArray()
        for (i in 0 until (tracks?.length() ?: 0)) {
            val t = tracks?.optJSONObject(i) ?: continue
            if ((t.optString("source") == DeviceMusic.SOURCE) == (mode == Prefs.MODE_DEVICE)) out.put(t)
        }
        return out
    }

    private fun describe(e: PlaybackException): String {
        val name = controller.currentMediaItem?.mediaMetadata?.title ?: "A track"
        return "$name: it could not be played (${e.errorCodeName})."
    }
}
