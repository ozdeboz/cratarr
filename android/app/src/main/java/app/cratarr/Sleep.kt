/*
 * Cratarr's own: the sleep timer, kept by this device's player, which alone is sure to be awake when it fires (behind a
 * locked screen, offline). At a time (epoch ms), or when the playing track ends: the player pauses, as the pause button
 * does, and the timer clears. The page and Play On set it (Prefs); the service applies it; both report it.
 * - A timer that runs out while the music is paused only clears; one left from before the service started is gone.
 * - Nothing queued any more (a stop, a handover away): no timer.
 */
package app.cratarr

import android.content.Context
import org.json.JSONObject

object Sleep {
    data class Timer(val atMs: Long?, val endOfTrack: Boolean)

    fun get(context: Context): Timer? = Prefs(context).sleep

    /** Sets it; neither a time nor the end of the track clears it. */
    fun set(context: Context, atMs: Long?, endOfTrack: Boolean) {
        Prefs(context).sleep = if (atMs == null && !endOfTrack) null else Timer(atMs, endOfTrack)
    }

    fun clear(context: Context) = set(context, null, false)

    /** As a command or a bridge message gives it: `at_ms`/`atMs` (null or missing: no time) and `end_of_track`/`endOfTrack`. */
    fun set(context: Context, m: JSONObject, at: String, end: String) =
        set(context, if (m.isNull(at)) null else m.optLong(at), m.optBoolean(end))

    /** For a report: `{at, end}` under the names given, or null when none is set. */
    fun json(context: Context, at: String, end: String): Any =
        get(context)?.let { JSONObject().put(at, it.atMs ?: JSONObject.NULL).put(end, it.endOfTrack) } ?: JSONObject.NULL
}
