/*
 * Cratarr's own (decision 0033, amended): repeat, applied by this device's player, which alone knows when a track ends
 * (and has to without a connection, without a gap).
 * - One: Media3's own repeat of the playing track; next and previous still go to another track, and it stays on.
 * - All: while the last track plays, the next round of the album or playlist goes after it (so it follows without a gap):
 *   its tracks by their place in it, those added by hand not again (they play once); shuffled anew when the queue plays
 *   shuffled, and not starting with the track that just played.
 * - A new play turns repeat one off; previous on the first track under repeat all goes to the last.
 */
package app.cratarr

import android.content.Context
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject

object Repeat {
    const val OFF = "off"
    const val ONE = "one"
    const val ALL = "all"


    fun mode(context: Context): String = Prefs(context).repeat

    /** Sets the mode and applies it. */
    fun set(context: Context, player: Player, mode: String, locate: (MediaItem) -> MediaItem) {
        val m = if (mode == ONE || mode == ALL) mode else OFF
        Prefs(context).repeat = m
        player.repeatMode = if (m == ONE) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
        // Repeat all turns Autoplay off (decision 0036): both say what comes after the queue. Radio lives with it.
        if (m == ALL && Autoplay.get(context) != null) Autoplay.set(context, player, null)
        if (m == ALL) extend(context, player, locate)
    }

    /** Applied again (the service started): Media3's repeat mode from the saved one. */
    fun apply(context: Context, player: Player) {
        player.repeatMode = if (mode(context) == ONE) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
    }

    /** Something new plays: repeat one goes off, repeat all stays. */
    fun newPlay(context: Context, player: Player) {
        if (mode(context) == ONE) {
            Prefs(context).repeat = OFF
            player.repeatMode = Player.REPEAT_MODE_OFF
        }
    }

    /** Previous on the first track under repeat all: to the last one. True when it did. */
    fun previous(context: Context, player: Player): Boolean {
        if (mode(context) != ALL || player.mediaItemCount < 2) return false
        if (player.currentMediaItemIndex != 0 || player.currentPosition > PlayerService.RESTART_AFTER_MS) return false
        player.seekTo(player.mediaItemCount - 1, 0)
        return true
    }

    /** Under repeat all, while the last track plays: the next round of the album or playlist after it. */
    fun extend(context: Context, player: Player, locate: (MediaItem) -> MediaItem) {
        val count = player.mediaItemCount
        if (mode(context) != ALL || count == 0 || player.currentMediaItemIndex != count - 1) return
        val tracks = (0 until count).mapNotNull { trackOf(player.getMediaItemAt(it)) }
        // The album or playlist: its tracks by their place in it (the last time each played), not those added by hand.
        val byPlace = sortedMapOf<Int, JSONObject>()
        // Only the album's or playlist's own tracks: none of a lane (added by hand, Radio's, Autoplay's).
        val own = { t: JSONObject -> t.optString("lane").isEmpty() }
        for (t in tracks) if (own(t) && t.has("n")) byPlace[t.optInt("n")] = t
        val round = if (byPlace.isNotEmpty()) {
            byPlace.values.toMutableList()
        } else {
            // A queue without places (played before they were kept): its own tracks, once each, in their order.
            tracks.filter(own).distinctBy { "${it.optString("source")}/${it.optString("id")}" }.toMutableList()
        }
        if (round.isEmpty()) return
        if (Prefs(context).shuffled) {
            round.shuffle()
            // A round does not start with the track that just played.
            val last = tracks.last()
            if (round.size > 1 && same(round[0], last)) round.add(round.removeAt(0))
        }
        // New entries for the new round's tracks; their places stay.
        val copies = JSONArray(round.map { JSONObject(it.toString()).apply { remove("entry") } })
        player.addMediaItems(mediaItems(copies).map(locate))
    }

    private fun same(a: JSONObject, b: JSONObject) = a.optString("source") == b.optString("source") && a.optString("id") == b.optString("id")
}
