/*
 * Cratarr's own (decision 0036): the radio, kept by this device's player, which fills its lane: when fewer than three
 * tracks are left ahead, Cratarr's scorer gives the next five (`POST /api/radio/next`), appended after everything with
 * lane "radio". Without a connection it chooses from this device's downloads (the playing and seed artists first).
 * - On: a mode (Similar by default) and a seed (the "Start radio" item's tracks; empty: the queue's own); what a
 *   change does not give stays. On turns repeat all off, and repeat all turns it off: both say what comes after.
 * - Off: its tracks still to come go.
 * - A new play keeps it on, its seed cleared (the new album or playlist is the seed), unless the play is radio tracks
 *   only (a radio starting from an item, which brings its own seed).
 */
package app.cratarr

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

object Radio {
    const val LANE = "radio"
    private const val TAG = "CratarrRadio"

    /** Asked for at a time, and how few may be left ahead before more are. */
    private const val BATCH = 5
    private const val LOW = 3

    /** `within`: a crate's id, its tracks the radio's only choice ("Play as radio"); null: the whole library. */
    data class State(val mode: String, val seed: List<String>, val within: String? = null)

    private val asks = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    /** Set while the next tracks are being asked for: one ask at a time. */
    @Volatile private var filling = false

    fun get(context: Context): State? = Prefs(context).radio

    /** On (`mode`, `seed` and `within`: as given, else as they were; `within` "" leaves the crate) or off (its tracks to
     *  come go). */
    fun set(context: Context, player: Player, on: Boolean, mode: String?, seed: List<String>?, within: String? = null) {
        if (!on) {
            Prefs(context).radio = null
            dropAhead(player)
            return
        }
        val was = get(context)
        val crate = when (within) {
            null -> was?.within
            "" -> null
            else -> within
        }
        Prefs(context).radio = State(mode ?: was?.mode ?: "similar", seed ?: was?.seed ?: emptyList(), crate)
        if (Repeat.mode(context) == Repeat.ALL) Repeat.set(context, player, Repeat.OFF) { it }
        // Another mode: what the old one chose makes way for what this one chooses.
        if (mode != null && was != null && mode != was.mode) dropAhead(player)
    }

    /** As a bridge message or a command gives it: `on`, `mode`, `seed` (left out: as it was). */
    fun set(context: Context, player: Player, m: JSONObject) {
        val seed = m.optJSONArray("seed")?.let { a -> (0 until a.length()).map { a.optString(it) }.filter { it.isNotEmpty() } }
        // Left out: as it was; null or "": out of the crate.
        val within = if (!m.has("within")) null else if (m.isNull("within")) "" else m.optString("within")
        set(context, player, m.optBoolean("on"), m.optString("mode").takeIf { it.isNotEmpty() && it != "null" }, seed, within)
    }

    /** Something new plays (`tracks`, as the page or Cratarr sent them): the radio stays on, its seed and crate cleared
     *  unless these are radio tracks only. */
    fun newPlay(context: Context, tracks: JSONArray?) {
        val r = get(context) ?: return
        val all = tracks != null && tracks.length() > 0 && (0 until tracks.length()).all { tracks.optJSONObject(it)?.optString("lane") == LANE }
        if (!all && (r.seed.isNotEmpty() || r.within != null)) Prefs(context).radio = r.copy(seed = emptyList(), within = null)
    }

    /** For a report: `{on, mode, seed}`. */
    fun json(context: Context): JSONObject {
        val r = get(context) ?: return JSONObject().put("on", false)
        return JSONObject().put("on", true).put("mode", r.mode).put("seed", JSONArray(r.seed)).put("within", r.within ?: JSONObject.NULL)
    }

    /** The radio's tracks still to come, out. */
    fun dropAhead(player: Player) {
        for (i in player.mediaItemCount - 1 downTo player.currentMediaItemIndex + 1) {
            if (trackOf(player.getMediaItemAt(i))?.optString("lane") == LANE) player.removeMediaItem(i)
        }
    }

    /** With the radio on and fewer than three tracks ahead: the next ones, appended (asked of Cratarr, else chosen
     *  from the downloads). `locate` gives an item its file. Called on the main thread. */
    fun fill(context: Context, player: Player, server: String?, locate: (MediaItem) -> MediaItem) {
        val r = get(context) ?: return
        val count = player.mediaItemCount
        if (filling || count == 0 || count - player.currentMediaItemIndex - 1 >= LOW) return
        val tracks = (0 until count).mapNotNull { trackOf(player.getMediaItemAt(it)) }
        val ids = tracks.map { it.optString("id") }
        val own = tracks.filter { it.optString("lane").isEmpty() }.map { it.optString("id") }
        val playing = player.currentMediaItem?.let(::trackOf)
        val ask = JSONObject()
            .put("mode", r.mode)
            .put("seed", JSONArray(r.seed.ifEmpty { own }))
            .put("exclude", JSONArray(ids))
            .put("last", playing?.optString("id") ?: JSONObject.NULL)
            .put("count", BATCH)
            .put("within", r.within ?: JSONObject.NULL)
        val app = context.applicationContext
        filling = true
        asks.execute {
            val got = server?.let { ask(it, ask) } ?: offline(app, r, tracks, playing)
            main.post {
                filling = false
                // Still on: appended after everything, so the music goes on into them.
                if (get(app) == null || got.length() == 0) return@post
                player.addMediaItems(mediaItems(got).map(locate))
            }
        }
    }

    /** Cratarr's choice; null without an answer (offline). */
    private fun ask(server: String, body: JSONObject): JSONArray? = try {
        val c = URL("${server.trimEnd('/')}/api/radio/next").openConnection() as HttpURLConnection
        c.requestMethod = "POST"
        c.connectTimeout = 5_000
        c.readTimeout = 15_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
        val ok = c.responseCode in 200..299
        val text = if (ok) c.inputStream.bufferedReader().use { it.readText() } else null
        c.disconnect()
        text?.let { JSONObject(it).optJSONArray("tracks") }
    } catch (e: Exception) {
        Log.w(TAG, "radio: ${e.message}")
        null
    }

    /** Without a connection: from this device's downloads, those not in the queue; the playing track's and the seed's
     *  artists first (Random and Reshuffle: any), then the rest, each group in a random order. A crate's bound is not
     *  known offline: the downloads are the bound then. */
    private fun offline(context: Context, r: State, queue: List<JSONObject>, playing: JSONObject?): JSONArray {
        val inQueue = queue.map { "${it.optString("source")}/${it.optString("id")}" }.toSet()
        val downloaded = Downloads.list(context).optJSONArray("tracks") ?: JSONArray()
        val candidates = (0 until downloaded.length())
            .mapNotNull { downloaded.optJSONObject(it) }
            .filter { it.optJSONObject("download")?.optString("state") == "done" }
            .filter { "${it.optString("source")}/${it.optString("id")}" !in inQueue }
            .shuffled()
        val artists = buildSet {
            playing?.optString("artist")?.takeIf { it.isNotEmpty() }?.let(::add)
            for (t in queue) if (t.optString("id") in r.seed) t.optString("artist").takeIf { it.isNotEmpty() }?.let(::add)
        }
        val ordered = if (r.mode == "random" || r.mode == "reshuffle") candidates else candidates.sortedBy { if (it.optString("artist") in artists) 0 else 1 }
        return JSONArray(ordered.take(BATCH).map { JSONObject(it.toString()).apply { remove("download"); remove("added"); remove("entry"); remove("n"); put("lane", LANE) } })
    }
}
