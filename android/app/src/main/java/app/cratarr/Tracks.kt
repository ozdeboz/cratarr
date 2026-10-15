package app.cratarr

import android.net.Uri
import android.os.Bundle
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject

/** The page's own description of a track (`source`, `id`, `name`, `artist`, `album`, `album_id`), kept with its item. */
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
        fun text(key: String) = t.optString(key).takeIf { it.isNotEmpty() && it != "null" }
        MediaItem.Builder()
            .setMediaId("${Uri.encode(source)}/${Uri.encode(id)}")
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(t.optString("name"))
                    .setArtist(text("artist"))
                    .setAlbumTitle(text("album"))
                    .setExtras(Bundle().apply { putString(TRACK, t.toString()) })
                    .build(),
            )
            .build()
    }
}

/** The player's queue, as the page and Cratarr describe tracks. */
fun queueOf(player: Player): JSONArray {
    val queue = JSONArray()
    for (i in 0 until player.mediaItemCount) {
        val track = player.getMediaItemAt(i).mediaMetadata.extras?.getString(TRACK)
        queue.put(track?.let(::JSONObject) ?: JSONObject())
    }
    return queue
}
