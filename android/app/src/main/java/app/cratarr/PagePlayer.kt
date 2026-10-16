package app.cratarr

import android.content.Context
import android.content.SharedPreferences

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaController
import org.json.JSONArray
import org.json.JSONObject

/**
 * Between the page's local player and the app's own (decision 0031, amended). The page's calls (`{"type": "player",
 * "op": …}`) become the player's; what the player does goes back to the page as `{"type": "player", "index",
 * "positionMs", "durationMs", "paused", "error", "queue"?}`. The queue's tracks are the page's own (source, id, name,
 * artist, album, album_id), kept with each media item, so a queue the player changed itself (Play On) reaches the page
 * as it is.
 */
@OptIn(UnstableApi::class)
class PagePlayer(private val context: Context, private val controller: MediaController, private val send: (String) -> Unit) : Player.Listener {
    /** Hold-to-preview (the queue's covers): a second player for the held track, while the music waits. */
    private var previewer: ExoPlayer? = null
    /** The music played when the preview began: it goes on after. */
    private var resumeAfter = false

    /** The sleep timer changed (set, or done): the page is told. Held here, as Android keeps listeners only weakly. */
    private val sleepChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == Prefs.SLEEP || key == Prefs.RADIO_ON || key == Prefs.RADIO_BY || key == Prefs.RADIO_COUNT || key == Prefs.AUTOPLAY) report()
    }

    init {
        controller.addListener(this)
        Prefs.shared(context).registerOnSharedPreferenceChangeListener(sleepChanged)
    }

    fun release() {
        previewer?.release()
        previewer = null
        controller.removeListener(this)
        Prefs.shared(context).unregisterOnSharedPreferenceChangeListener(sleepChanged)
    }

    fun handle(m: JSONObject) {
        val c = controller
        when (m.optString("op")) {
            "queue" -> {
                // A new play turns repeat one off; whether it comes shuffled counts for repeat all's next rounds.
                Repeat.newPlay(context, c)
                Autoplay.newPlay(context, m.optJSONArray("tracks"))
                Prefs(context).shuffled = m.optBoolean("shuffled")
                c.setMediaItems(mediaItems(m.optJSONArray("tracks")), m.optInt("index", 0), m.optLong("positionMs", 0))
                c.prepare()
                c.playWhenReady = m.optBoolean("play", true)
            }
            "add" -> c.addMediaItems(m.optInt("at", c.mediaItemCount), mediaItems(m.optJSONArray("tracks")))
            "move" -> c.moveMediaItem(m.optInt("from"), m.optInt("to"))
            "remove" -> c.removeMediaItem(m.optInt("index"))
            // Everything after the playing track replaced (the shuffle switch); the playing track plays on.
            "upcoming" -> {
                if (m.has("shuffled")) Prefs(context).shuffled = m.optBoolean("shuffled")
                val from = (c.currentMediaItemIndex + 1).coerceAtMost(c.mediaItemCount)
                c.replaceMediaItems(from, c.mediaItemCount, mediaItems(m.optJSONArray("tracks")))
            }
            "jump" -> {
                c.seekTo(m.optInt("index"), 0)
                play()
            }
            "next" -> c.seekToNextMediaItem()
            "previous" -> c.seekToPrevious()
            // Its items are located by the service when they reach it.
            "repeat" -> Repeat.set(context, c, m.optString("mode")) { it }
            // Kept by the service (Sleep); the change reaches the page as a report.
            "sleep" -> Sleep.set(context, m, "atMs", "endOfTrack")
            // Kept and filled by the service (Radio); the change reaches the page as a report.
            "radio" -> Radio.command(context, c, m)
            "autoplay" -> Autoplay.command(context, c, m)
            "seek" -> c.seekTo(m.optLong("positionMs"))
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

    /** The held track from `positionMs`, the music paused meanwhile. */
    private fun preview(track: JSONObject?, positionMs: Long) {
        val item = track?.let { mediaItems(JSONArray().put(it)).firstOrNull() } ?: return
        val located = Downloads.locate(context, item, Prefs(context).serverUrl?.trimEnd('/'))
        val p = previewer ?: ExoPlayer.Builder(context)
            .setMediaSourceFactory(DefaultMediaSourceFactory(Downloads.playback(context)))
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
        val out = JSONObject()
            .put("type", "player")
            .put("index", if (c.mediaItemCount == 0) -1 else c.currentMediaItemIndex)
            .put("positionMs", c.currentPosition)
            .put("durationMs", if (c.duration == C.TIME_UNSET) -1 else c.duration)
            .put("paused", !c.playWhenReady || c.playbackState == Player.STATE_ENDED)
            .put("error", c.playerError?.let(::describe) ?: JSONObject.NULL)
            .put("repeat", Repeat.mode(context))
            .put("sleep", Sleep.json(context, "atMs", "endOfTrack"))
            .put("radio", Radio.on(context))
            .put("radioBy", Radio.by(context))
            .put("radioCount", Radio.count(context))
            .put("autoplay", Autoplay.json(context))
        if (withQueue) out.put("queue", queueOf(c))
        send(out.toString())
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

    private fun describe(e: PlaybackException): String {
        val name = controller.currentMediaItem?.mediaMetadata?.title ?: "A track"
        return "$name: it could not be played (${e.errorCodeName})."
    }
}
