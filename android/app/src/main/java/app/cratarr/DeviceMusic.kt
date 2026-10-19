/*
 * Local mode (decision 0052): the device's own music, as Android indexes it (MediaStore: its music, not ringtones,
 * alarms or recordings), played by the app's player in a mode of its own, never mixed with Cratarr's. Cratarr's server
 * keeps nothing of it: the page asks the app for the tracks, their covers come from Android, and the player reads the
 * files through Android.
 */
package app.cratarr

import android.Manifest
import android.content.ContentUris
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.util.Size
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.util.BitmapLoader
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.util.concurrent.Executors

object DeviceMusic {
    /** Its tracks' source in the page's and the player's descriptions. */
    const val SOURCE = "device"

    /** Android's permission to read the device's music. */
    val permission: String = if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_AUDIO else Manifest.permission.READ_EXTERNAL_STORAGE

    fun granted(context: Context): Boolean = ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    /** `granted`, `denied` (asked, refused) or `unasked`: what the page shows before local mode is opened. */
    fun state(context: Context): String = when {
        granted(context) -> "granted"
        Prefs(context).deviceMusicAsked -> "denied"
        else -> "unasked"
    }

    /** Local mode is there: Android lets the app read the device's music, and at least one of its folders is on. */
    fun on(context: Context): Boolean = granted(context) && Prefs(context).localFolders.isNotEmpty()

    /**
     * The folders Android found music in, by their top folder ("Music/Artist/Album/" is "Music/"), as `{path, name,
     * tracks, on}`, the most tracks first. One turned on that holds nothing now still shows, to be turned off.
     */
    fun folders(context: Context): JSONArray {
        val on = Prefs(context).localFolders
        val counts = HashMap<String, Int>()
        context.contentResolver.query(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, arrayOf(MediaStore.Audio.Media.RELATIVE_PATH), MUSIC, null, null)?.use { c ->
            while (c.moveToNext()) {
                val path = c.getString(0)?.takeIf { it.isNotEmpty() } ?: continue
                val top = path.substringBefore('/') + "/"
                counts[top] = (counts[top] ?: 0) + 1
            }
        }
        for (f in on) counts.putIfAbsent(f, 0)
        return JSONArray(
            counts.entries.sortedWith(compareByDescending<Map.Entry<String, Int>> { it.value }.thenBy { it.key }).map { (path, n) ->
                JSONObject().put("path", path).put("name", path.trimEnd('/')).put("tracks", n).put("on", path in on)
            },
        )
    }

    /** Whether an item is one of the device's own tracks. */
    fun owns(item: MediaItem): Boolean = item.mediaId.startsWith("$SOURCE/")

    /** The file of a track, by its id in Android's index. */
    fun uri(id: String): Uri? = id.toLongOrNull()?.let { ContentUris.withAppendedId(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, it) }

    /** An album's cover, as the player's notification and lock screen ask for it. */
    fun coverUri(albumId: String): Uri? = albumId.toLongOrNull()?.let { ContentUris.withAppendedId(MediaStore.Audio.Albums.EXTERNAL_CONTENT_URI, it) }

    /** One of the device's own tracks with its file and its album's cover from Android; null for any other. */
    fun located(item: MediaItem): MediaItem? {
        if (!owns(item)) return null
        val id = Uri.decode(item.mediaId.removePrefix("$SOURCE/"))
        val album = trackOf(item)?.optString("album_id").orEmpty()
        val b = item.buildUpon()
        uri(id)?.let(b::setUri)
        coverUri(album)?.let { b.setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(it).build()) }
        return b.build()
    }

    /** Every music track in the folders turned on, as the page describes tracks (`source` device): newest first. */
    fun tracks(context: Context): JSONArray {
        val out = JSONArray()
        val on = Prefs(context).localFolders.toList()
        if (on.isEmpty()) return out
        val inFolders = on.joinToString(" OR ", "(", ")") { "${MediaStore.Audio.Media.RELATIVE_PATH} LIKE ? ESCAPE '\\'" }
        val folders = on.map { it.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%" }.toTypedArray()
        val cols = arrayOf(
            MediaStore.Audio.Media._ID, MediaStore.Audio.Media.TITLE, MediaStore.Audio.Media.ARTIST, MediaStore.Audio.Media.ALBUM,
            MediaStore.Audio.Media.ALBUM_ID, MediaStore.Audio.Media.TRACK, MediaStore.Audio.Media.YEAR, MediaStore.Audio.Media.DURATION,
            MediaStore.Audio.Media.DATE_ADDED,
        )
        context.contentResolver.query(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, cols, "$MUSIC AND $inFolders", folders, "${MediaStore.Audio.Media.DATE_ADDED} DESC")?.use { c ->
            fun text(i: Int) = c.getString(i)?.trim()?.takeIf { it.isNotEmpty() && it != MediaStore.UNKNOWN_STRING }
            while (c.moveToNext()) {
                // Android keeps the disc in the thousands of the track number.
                val track = c.getInt(5)
                out.put(
                    JSONObject()
                        .put("source", SOURCE)
                        .put("id", c.getLong(0).toString())
                        .put("name", text(1) ?: "")
                        .put("artist", text(2) ?: JSONObject.NULL)
                        .put("album", text(3) ?: JSONObject.NULL)
                        .put("album_id", c.getLong(4).toString())
                        // Its number on the album: not `n`, which is a track's place in what plays.
                        .put("track", if (track > 0) track % 1000 else JSONObject.NULL)
                        .put("disc", if (track >= 1000) track / 1000 else JSONObject.NULL)
                        .put("year", c.getInt(6).takeIf { it > 0 } ?: JSONObject.NULL)
                        .put("duration_secs", c.getLong(7) / 1000.0)
                        .put("added", c.getLong(8)),
                )
            }
        }
        return out
    }

    /** Music, not ringtones, alarms or recordings. */
    private val MUSIC = "${MediaStore.Audio.Media.IS_MUSIC} != 0"

    /** An album's cover as JPEG, from Android; null where it has none. */
    fun cover(context: Context, albumId: String, size: Int = 512): ByteArray? = coverBitmap(context, albumId, size)?.let { b ->
        ByteArrayOutputStream().use { out ->
            b.compress(Bitmap.CompressFormat.JPEG, 88, out)
            out.toByteArray()
        }
    }

    private fun coverBitmap(context: Context, albumId: String, size: Int): Bitmap? = try {
        coverUri(albumId)?.let { context.contentResolver.loadThumbnail(it, Size(size, size), null) }
    } catch (e: Exception) {
        null
    }

    /**
     * Covers for the player's notification and lock screen: the device's own albums from Android, every other picture
     * as before ([other]).
     */
    @UnstableApi
    class Covers(private val context: Context, private val other: BitmapLoader) : BitmapLoader {
        private val reading = MoreExecutors.listeningDecorator(Executors.newSingleThreadExecutor())

        override fun supportsMimeType(mimeType: String): Boolean = other.supportsMimeType(mimeType)

        override fun decodeBitmap(data: ByteArray): ListenableFuture<Bitmap> = other.decodeBitmap(data)

        override fun loadBitmap(uri: Uri): ListenableFuture<Bitmap> {
            val album = uri.takeIf { it.authority == MediaStore.AUTHORITY && it.pathSegments.contains("albums") }?.lastPathSegment
                ?: return other.loadBitmap(uri)
            return reading.submit<Bitmap> { coverBitmap(context, album, 512) ?: error("no cover for album $album") }
        }
    }
}
