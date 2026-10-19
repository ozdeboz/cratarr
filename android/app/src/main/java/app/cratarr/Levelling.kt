/*
 * Levelling (decision 0035): every track about as loud as the next, as ReplayGain does. Each track's gain comes from
 * Cratarr (`/api/track/{source}/{id}/gain`: its analysis, else its file's own tags): the album's while an album plays
 * in its own order (a quiet interlude stays quiet), the track's otherwise. Never past the track's peak (no clipping)
 * and never above full volume: the player can only lower. A track whose gain is not known gets a typical one, so it
 * does not stand out among levelled tracks. Applied as part of the player's volume, which Android applies as the
 * track plays: the change falls between the tracks.
 */
package app.cratarr

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.util.LruCache
import androidx.media3.common.Player
import org.json.JSONObject
import java.util.concurrent.Executors
import kotlin.math.min
import kotlin.math.pow

object Levelling {
    /** A track's gain when nothing is known of it: about what a modern master needs. */
    private const val UNKNOWN_DB = -6.0

    /** The playing track's part of the volume (0 to 1). */
    @Volatile var factor = 1f
        private set

    private val gains = LruCache<String, JSONObject>(1000)
    private val asks = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    /** The track looked at last (its key), so a report does not ask again. */
    private var following: String? = null

    fun on(context: Context): Boolean = Prefs(context).levelling

    /** The playing track (or the setting) changed: its gain, then `apply` (main thread) to set the volume again. */
    fun follow(context: Context, player: Player, server: String?, apply: () -> Unit) {
        val item = player.currentMediaItem
        val track = item?.let(::trackOf)
        // The device's own music (decision 0052) has no gain from Cratarr.
        if (!on(context) || track == null || track.optString("source") == DeviceMusic.SOURCE) {
            following = null
            if (factor != 1f) {
                factor = 1f
                apply()
            }
            return
        }
        val key = "${track.optString("source")}/${track.optString("id")}"
        val albumWise = inAlbumOrder(context, player)
        val want = "$key:$albumWise"
        if (want == following) return
        following = want
        val known = gains.get(key)
        if (known != null) {
            set(known, albumWise, apply)
            return
        }
        val base = server?.trimEnd('/') ?: return
        asks.execute {
            val g = ask(context, "$base/api/track/$key/gain")
            main.post {
                if (g != null) gains.put(key, g)
                // Still that track.
                if (following == want) set(g ?: JSONObject(), albumWise, apply)
            }
        }
    }

    private fun set(g: JSONObject, albumWise: Boolean, apply: () -> Unit) {
        val gain = (if (albumWise) g.optDouble("albumGain", Double.NaN) else Double.NaN).takeUnless { it.isNaN() }
            ?: g.optDouble("trackGain", Double.NaN).takeUnless { it.isNaN() } ?: UNKNOWN_DB
        val peak = (if (albumWise) g.optDouble("albumPeak", Double.NaN) else Double.NaN).takeUnless { it.isNaN() }
            ?: g.optDouble("trackPeak", Double.NaN).takeUnless { it.isNaN() } ?: 1.0
        val f = min(min(10.0.pow(gain / 20.0), if (peak > 0) 1.0 / peak else 1.0), 1.0).toFloat()
        if (f != factor) {
            factor = f
            apply()
        }
    }

    /** An album plays in its own order: the queue's own tracks around the playing one are of its album, unshuffled. */
    private fun inAlbumOrder(context: Context, player: Player): Boolean {
        if (Prefs(context).shuffled) return false
        val i = player.currentMediaItemIndex
        val album = player.currentMediaItem?.let(::trackOf)?.optString("album_id")?.takeIf { it.isNotEmpty() } ?: return false
        fun same(j: Int) = j in 0 until player.mediaItemCount &&
            trackOf(player.getMediaItemAt(j))?.let { it.optString("album_id") == album && it.optString("lane").isEmpty() } == true
        return same(i - 1) || same(i + 1)
    }

    private fun ask(context: Context, url: String): JSONObject? = try {
        val c = Net.open(context, url)
        c.connectTimeout = 4_000
        c.readTimeout = 4_000
        val text = if (Net.code(context, c) in 200..299) c.inputStream.bufferedReader().use { it.readText() } else null
        c.disconnect()
        text?.let { JSONObject(it) }
    } catch (e: Exception) {
        Log.w("CratarrLevelling", "gain: ${e.message}")
        null
    }
}
