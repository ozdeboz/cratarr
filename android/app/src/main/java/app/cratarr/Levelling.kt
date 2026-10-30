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

    /** Switching levelling on or off is never a hard edge (0.9.12, the owner): the track goes to its new level over this,
     *  even in dB. */
    private const val RAMP_MS = 1_000L
    private var rampFrom = 1f
    private var rampTo = 1f
    private var rampAt = 0L
    private var rampApply: (() -> Unit)? = null
    private val ramper = object : Runnable {
        override fun run() {
            val t = ((android.os.SystemClock.elapsedRealtime() - rampAt).toFloat() / RAMP_MS).coerceIn(0f, 1f)
            val eased = ((1 - kotlin.math.cos(Math.PI * t)) / 2).toFloat()
            val db = db(rampFrom) + (db(rampTo) - db(rampFrom)) * eased
            factor = if (t >= 1f) rampTo else 10f.pow(db / 20f)
            rampApply?.invoke()
            if (t < 1f) main.postDelayed(this, 16L)
        }
    }

    private fun db(f: Float): Float = 20f * kotlin.math.log10(f.coerceAtLeast(1e-4f))

    /** To [f]: eased over a second ([ramp]: the setting changed), else at once (another track). */
    private fun setFactor(f: Float, ramp: Boolean, apply: () -> Unit) {
        main.removeCallbacks(ramper)
        if (f == factor) return
        if (!ramp) {
            factor = f
            apply()
            return
        }
        rampFrom = factor
        rampTo = f
        rampAt = android.os.SystemClock.elapsedRealtime()
        rampApply = apply
        main.post(ramper)
    }

    /** What that turns the track down by, in dB (0 or less; 0 when off): the page shows it (0.9.8). */
    val appliedDb: Double
        get() = if (factor >= 1f) 0.0 else Math.round(20.0 * kotlin.math.log10(factor.toDouble()) * 100.0) / 100.0

    private val gains = LruCache<String, JSONObject>(1000)
    private val asks = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    /** The track looked at last (its key), so a report does not ask again. */
    private var following: String? = null

    fun on(context: Context): Boolean = Prefs(context).levelling

    /** The playing track (or the setting) changed: its gain, then `apply` (main thread) to set the volume again. */
    fun follow(context: Context, player: Player, server: String?, apply: () -> Unit, ramp: Boolean = false) {
        val item = player.currentMediaItem
        val track = item?.let(::trackOf)
        // The device's own music (decision 0052) has no gain from Cratarr.
        if (!on(context) || track == null || track.optString("source") == DeviceMusic.SOURCE) {
            following = null
            setFactor(1f, ramp, apply)
            return
        }
        val key = "${track.optString("source")}/${track.optString("id")}"
        val albumWise = inAlbumOrder(context, player)
        // A track not measured (0.9.8, the page's This device › Player): typical, or left as it is.
        val unknownDb = if (Prefs(context).levelUnknown == Prefs.LEVEL_UNTOUCHED) 0.0 else UNKNOWN_DB
        val want = "$key:$albumWise:$unknownDb"
        if (want == following) return
        following = want
        val known = gains.get(key)
        if (known != null) {
            set(known, albumWise, unknownDb, apply, ramp)
            return
        }
        val base = server?.trimEnd('/') ?: return
        asks.execute {
            val g = ask(context, "$base/api/track/$key/gain")
            main.post {
                if (g != null) gains.put(key, g)
                // Still that track.
                if (following == want) set(g ?: JSONObject(), albumWise, unknownDb, apply, ramp)
            }
        }
    }

    private fun set(g: JSONObject, albumWise: Boolean, unknownDb: Double, apply: () -> Unit, ramp: Boolean) {
        // The dB Cratarr says to turn it down by (0.9.8: the rule kept on the server, the page shows the same); a
        // track not measured gets the device's choice.
        fun down(key: String) = if (g.isNull(key)) Double.NaN else g.optDouble(key, Double.NaN)
        val db = (if (albumWise) down("albumDown") else Double.NaN).takeUnless { it.isNaN() }
            ?: down("trackDown").takeUnless { it.isNaN() } ?: unknownDb
        val f = min(10.0.pow(db / 20.0), 1.0).toFloat()
        setFactor(f, ramp, apply)
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
