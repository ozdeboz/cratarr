/*
 * Cratarr's own (decision 0033, amended): repeat, applied by this device's player, which alone knows when a track ends
 * (and has to without a connection, without a gap).
 * - One: Media3's own repeat of the playing track; next and previous still go to another track, and it stays on.
 * - All: while the last track plays, the next round of the album or playlist goes after it (so it follows without a gap):
 *   its tracks by their place in it, those added by hand not again (they play once); shuffled anew when the queue plays
 *   shuffled, and not starting with the track that just played.
 * - A new play turns repeat one off; previous on the first track under repeat all goes to the last.
 * What the next round holds is the rules crate's (`next_round`): Kotlin holds no decisions.
 */
package app.cratarr

import android.content.Context
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import app.cratarr.rules.QueueTrack
import app.cratarr.rules.nextRound
import org.json.JSONArray
import org.json.JSONObject
import kotlin.random.Random

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
        val queue = tracks.map { QueueTrack("${it.optString("source")}/${it.optString("id")}", if (it.has("n")) it.optInt("n").toUInt() else null, laneOf(it)) }
        val round = try {
            nextRound(queue, (tracks.size - 1).toUInt(), Prefs(context).shuffled, Random.nextLong(Long.MAX_VALUE shr 11).toULong())
        } catch (e: LinkageError) {
            return
        }
        if (round.isEmpty()) return
        // New entries for the new round's tracks; their places stay.
        val copies = JSONArray(round.map { JSONObject(tracks[it.toInt()].toString()).apply { remove("entry") } })
        player.addMediaItems(mediaItems(copies).map(locate))
    }
}
