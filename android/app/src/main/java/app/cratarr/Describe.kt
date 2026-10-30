package app.cratarr

import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

/**
 * Cratarr describes the tracks it is told of (0.9.6, decision 0033 amended): the queue this player reports is its
 * tracks' identities, and what the notification, the lock screen and a car show (title, artist, album) is what
 * Cratarr says of them (`POST /api/tracks/describe`), kept per track. A server from before it (404) is told the queue
 * with its details, as before.
 */
object Describe {
    private const val TAG = "CratarrDescribe"

    /** At most this many asked at once (the server's limit). */
    private const val MOST = 500

    /** The extras key `Tracks.kt` keeps a track's description under. */
    private const val TRACK = "cratarr.track"

    /** Whether the server describes tracks: null not asked yet, false a server from before. */
    @Volatile
    var available: Boolean? = null

    /** What Cratarr said of each track, by its item's id (`source/id`): name, artist, album. */
    private val kept = ConcurrentHashMap<String, Triple<String, String?, String?>>()
    private val asking = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    /** The sources Cratarr describes: its own library's tracks and Bandcamp's previews. */
    fun describes(source: String) = source == "cratarr" || source == "bandcamp"

    /** The player's tracks Cratarr describes, asked about once each, and shown as it says. On the main thread. */
    fun fill(context: Context, player: Player) {
        if (available == false) return
        val items = (0 until player.mediaItemCount).map(player::getMediaItemAt)
        val wanted = items.mapNotNull(::trackOf)
            .filter { describes(it.optString("source")) && !kept.containsKey(idOf(it)) }
            .map { it.optString("source") to it.optString("id") }
            .distinct()
        if (wanted.isEmpty()) return apply(player)
        val app = context.applicationContext
        asking.execute {
            var said: Boolean? = null
            for (part in wanted.chunked(MOST)) said = ask(app, part) ?: return@execute
            available = said != false
            main.post { apply(player) }
        }
    }

    private fun idOf(t: JSONObject) = "${Uri.encode(t.optString("source"))}/${Uri.encode(t.optString("id"))}"

    /** Each item shown as Cratarr said, where it says otherwise than the item does. */
    private fun apply(player: Player) {
        for (i in 0 until player.mediaItemCount) {
            val item = player.getMediaItemAt(i)
            val (name, artist, album) = kept[item.mediaId] ?: continue
            val m = item.mediaMetadata
            if (m.title?.toString() == name && m.artist?.toString() == artist && m.albumTitle?.toString() == album) continue
            // The description kept with it too, so the page and the plays read the same.
            val track = trackOf(item)?.put("name", name)?.put("artist", artist ?: JSONObject.NULL)?.put("album", album ?: JSONObject.NULL)
            val extras = Bundle(m.extras ?: Bundle()).apply { track?.let { putString(TRACK, it.toString()) } }
            val described = m.buildUpon().setTitle(name).setArtist(artist).setAlbumTitle(album).setExtras(extras).build()
            player.replaceMediaItem(i, item.buildUpon().setMediaMetadata(described).build())
        }
    }

    /** Cratarr asked about these tracks; false where it does not describe (a server from before), null on a failure. */
    private fun ask(context: Context, tracks: List<Pair<String, String>>): Boolean? {
        val server = Prefs(context).serverUrl?.trimEnd('/') ?: return null
        val body = JSONObject().put("tracks", JSONArray().apply { tracks.forEach { (s, id) -> put(JSONObject().put("source", s).put("id", id)) } })
        return try {
            val c = Net.open(context, "$server/api/tracks/describe")
            c.requestMethod = "POST"
            c.connectTimeout = 10_000
            c.readTimeout = 15_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            val code = Net.code(context, c)
            if (code == 404 || code == 405) {
                c.disconnect()
                return false
            }
            if (code !in 200..299) {
                c.disconnect()
                return null
            }
            val text = c.inputStream.bufferedReader().use { it.readText() }
            c.disconnect()
            val said = JSONObject(text).optJSONArray("tracks") ?: JSONArray()
            for (i in 0 until said.length()) {
                val t = said.optJSONObject(i) ?: continue
                if (!t.optBoolean("found")) continue
                fun text(key: String) = t.optString(key).takeIf { it.isNotEmpty() && it != "null" }
                kept[idOf(t)] = Triple(t.optString("name"), text("artist"), text("album"))
            }
            true
        } catch (e: Exception) {
            Log.w(TAG, "describe: ${e.message}")
            null
        }
    }
}
