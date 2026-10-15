package app.cratarr

import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import org.json.JSONObject

/**
 * Between the page's local player and the app's own (decision 0031, amended). The page's calls (`{"type": "player",
 * "op": …}`) become the player's; what the player does goes back to the page as `{"type": "player", "index",
 * "positionMs", "durationMs", "paused", "error", "queue"?}`. The queue's tracks are the page's own (source, id, name,
 * artist, album, album_id), kept with each media item, so a queue the player changed itself (Play On) reaches the page
 * as it is.
 */
class PagePlayer(private val controller: MediaController, private val send: (String) -> Unit) : Player.Listener {

    init {
        controller.addListener(this)
    }

    fun release() = controller.removeListener(this)

    fun handle(m: JSONObject) {
        val c = controller
        when (m.optString("op")) {
            "queue" -> {
                c.setMediaItems(mediaItems(m.optJSONArray("tracks")), m.optInt("index", 0), m.optLong("positionMs", 0))
                c.prepare()
                c.playWhenReady = m.optBoolean("play", true)
            }
            "add" -> c.addMediaItems(m.optInt("at", c.mediaItemCount), mediaItems(m.optJSONArray("tracks")))
            "move" -> c.moveMediaItem(m.optInt("from"), m.optInt("to"))
            "remove" -> c.removeMediaItem(m.optInt("index"))
            "jump" -> {
                c.seekTo(m.optInt("index"), 0)
                play()
            }
            "next" -> c.seekToNextMediaItem()
            "previous" -> c.seekToPrevious()
            "seek" -> c.seekTo(m.optLong("positionMs"))
            "play" -> play()
            "pause" -> c.pause()
            "toggle" -> if (c.playWhenReady && c.playbackState != Player.STATE_ENDED) c.pause() else play()
            "stop" -> {
                c.stop()
                c.clearMediaItems()
            }
        }
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

    override fun onPlayerError(error: PlaybackException) {
        // Skip what cannot play, as a player does, unless it was the last thing.
        val c = controller
        if (c.hasNextMediaItem()) {
            c.seekToNextMediaItem()
            c.prepare()
            c.play()
        }
    }

    private fun describe(e: PlaybackException): String {
        val name = controller.currentMediaItem?.mediaMetadata?.title ?: "A track"
        return "$name: it could not be played (${e.errorCodeName})."
    }
}
