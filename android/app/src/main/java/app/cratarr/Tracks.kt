package app.cratarr

import android.net.Uri
import android.os.Bundle
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject
import java.net.URLEncoder
import java.util.UUID

/**
 * The page's own description of a track (`entry`, `source`, `id`, `name`, `artist`, `album`, `album_id`), kept with its
 * item. `entry` is its entry in the queue (decision 0033): Cratarr's, or one made here for a track that came without.
 */
private const val TRACK = "cratarr.track"

/**
 * Media items for tracks as the page and Cratarr describe them, by id (`source/id`, each part URL-encoded). Their file
 * and cover are added by [PlayerService.located].
 */
fun mediaItems(tracks: JSONArray?): List<MediaItem> {
    if (tracks == null) return emptyList()
    return (0 until tracks.length()).mapNotNull { i ->
        val t = tracks.optJSONObject(i) ?: return@mapNotNull null
        val source = t.optString("source")
        val id = t.optString("id")
        if (source.isEmpty() || id.isEmpty()) return@mapNotNull null
        val track = if (t.optString("entry").isEmpty()) JSONObject(t.toString()).put("entry", UUID.randomUUID().toString()) else t
        fun text(key: String) = t.optString(key).takeIf { it.isNotEmpty() && it != "null" }
        MediaItem.Builder()
            .setMediaId("${Uri.encode(source)}/${Uri.encode(id)}")
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(t.optString("name"))
                    .setArtist(text("artist"))
                    .setAlbumTitle(text("album"))
                    .setExtras(Bundle().apply { putString(TRACK, track.toString()) })
                    .build(),
            )
            .build()
    }
}

/** An item's track, as the page described it. */
fun trackOf(item: MediaItem): JSONObject? = item.mediaMetadata.extras?.getString(TRACK)?.let {
    try {
        JSONObject(it)
    } catch (e: Exception) {
        null
    }
}

/** The copy a track of a Play from queue is heard from, a library's id; null: the one that plays for whoever asks. */
fun copyNamed(t: JSONObject?): String? = t?.optString("from")?.takeIf { it.isNotEmpty() && it != "null" }

/** An item's copy, as [copyNamed]. */
fun copyOf(item: MediaItem): String? = copyNamed(trackOf(item))

/** A copy as Cratarr's addresses ask for it. A library's id holds `/`, which must arrive whole. */
fun copyParam(library: String) = "from=" + URLEncoder.encode(library, "UTF-8").replace("+", "%20")

/** An item's entry in the queue. */
fun entryOf(item: MediaItem): String? =
    item.mediaMetadata.extras?.getString(TRACK)?.let { JSONObject(it).optString("entry") }?.takeIf { it.isNotEmpty() }

/** The place of an entry in the player's queue; -1 when it is not there. */
fun placeOf(player: Player, entry: String): Int = (0 until player.mediaItemCount).firstOrNull { entryOf(player.getMediaItemAt(it)) == entry } ?: -1

/** The player's queue, as the page and Cratarr describe tracks; [thin]: the tracks Cratarr describes as their identity
 *  only (0.9.6, decision 0033 amended). */
fun queueOf(player: Player, thin: Boolean = false): JSONArray {
    val queue = JSONArray()
    for (i in 0 until player.mediaItemCount) {
        val track = player.getMediaItemAt(i).mediaMetadata.extras?.getString(TRACK)?.let(::JSONObject) ?: JSONObject()
        queue.put(if (thin && Describe.describes(track.optString("source"))) identityOf(track) else track)
    }
    return queue
}

/** What makes a queue entry what it is, whatever Cratarr says of its track: its entry, lane and place, the track, who
 *  asked for it (a queue request accepted, 0.10), and the copy it is heard from (Play from). */
private val IDENTITY = listOf("entry", "lane", "n", "source", "id", "asked_by", "from")

/** A track's identity: what the queue is told when Cratarr describes it. */
fun identityOf(t: JSONObject): JSONObject = JSONObject().apply { for (k in IDENTITY) if (t.has(k)) put(k, t.get(k)) }
