/*
 * Cratarr's own (decision 0036, amended: Radio and Autoplay apart, as Symfonium's). Both are kept by this device's
 * player, which alone knows when a track starts and how many are left, offline too.
 * - Radio inserts: while on, each of the queue's own tracks (no lane) that starts playing gets about five tracks like it
 *   right after it and its run of tracks added by hand, in the "radio" lane; off drops those not yet played.
 * - Autoplay appends: when fewer than three tracks are left ahead, the next five by its mode (lane "autoplay"); with
 *   Restart, the queue's own tracks again once they have all played. Stop drops those not yet played.
 * Cratarr's scorer chooses (`POST /api/radio/next`); without a connection, this device's downloads (the playing and seed
 * artists first). Autoplay and repeat all exclude each other; radio lives with both.
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

/** What both ask of Cratarr, and how they choose without it. */
private object Scorer {
    const val TAG = "CratarrRadio"
    val asks = Executors.newSingleThreadExecutor()
    val main = Handler(Looper.getMainLooper())

    /** Cratarr's choice; null without an answer (offline). */
    fun ask(server: String, body: JSONObject): JSONArray? = try {
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

    /** Without a connection: from the downloads not in the queue, `artists` first (`any`: in any order), at most `count`,
     *  in `lane`. */
    fun offline(context: Context, queue: List<JSONObject>, artists: Set<String>, any: Boolean, count: Int, lane: String): JSONArray {
        val inQueue = queue.map { "${it.optString("source")}/${it.optString("id")}" }.toSet()
        val downloaded = Downloads.list(context).optJSONArray("tracks") ?: JSONArray()
        val candidates = (0 until downloaded.length())
            .mapNotNull { downloaded.optJSONObject(it) }
            .filter { it.optJSONObject("download")?.optString("state") == "done" }
            .filter { "${it.optString("source")}/${it.optString("id")}" !in inQueue }
            .shuffled()
        val ordered = if (any) candidates else candidates.sortedBy { if (it.optString("artist") in artists) 0 else 1 }
        return JSONArray(ordered.take(count).map { JSONObject(it.toString()).apply { remove("download"); remove("added"); remove("entry"); remove("n"); put("lane", lane) } })
    }

    fun tracksOf(player: Player) = (0 until player.mediaItemCount).mapNotNull { trackOf(player.getMediaItemAt(it)) }

    /** Drops the tracks of `lane` still to come. */
    fun dropAhead(player: Player, lane: String) {
        for (i in player.mediaItemCount - 1 downTo player.currentMediaItemIndex + 1) {
            if (trackOf(player.getMediaItemAt(i))?.optString("lane") == lane) player.removeMediaItem(i)
        }
    }
}

object Radio {
    const val LANE = "radio"
    private const val COUNT = 5

    /** The playing track's entry asked for, while it is being asked: one ask per track. */
    @Volatile private var asking: String? = null

    fun on(context: Context): Boolean = Prefs(context).radioOn

    fun set(context: Context, player: Player, on: Boolean) {
        Prefs(context).radioOn = on
        if (!on) Scorer.dropAhead(player, LANE)
    }

    /** A `radio` op or command. One with a mode, seed or bound is the older shape: Autoplay (decision 0036, amended); a
     *  bare off switches off both, so "radio off" still stops what Home Assistant started. */
    fun command(context: Context, player: Player, m: JSONObject) {
        if (m.has("mode") || m.has("seed") || m.has("within")) {
            val old = m.optString("mode")
            val mode = when (old) {
                "", "null", "continuous", "album_mix", "reshuffle" -> "similar"
                else -> old
            }
            val fields = JSONObject(m.toString()).put("mode", if (m.optBoolean("on")) mode else "stop")
            if (old == "album_mix") fields.put("albums", true)
            if (old == "reshuffle") fields.put("restart", true)
            Autoplay.command(context, player, fields)
            return
        }
        val on = m.optBoolean("on")
        set(context, player, on)
        if (!on) Autoplay.set(context, player, null)
    }

    /** With radio on, the playing track one of the queue's own: tracks like it after it and its hand-added run, unless
     *  they are there already (a seek, a replay, a report). Called on the main thread. */
    fun insert(context: Context, player: Player, server: String?, locate: (MediaItem) -> MediaItem) {
        if (!on(context)) return
        val c = player.currentMediaItemIndex
        val count = player.mediaItemCount
        if (c < 0 || c >= count) return
        val playing = trackOf(player.getMediaItemAt(c)) ?: return
        if (playing.optString("lane").isNotEmpty()) return
        val key = playing.optString("entry").ifEmpty { "$c" }
        if (asking == key || placeFor(player) == null) return
        val queue = Scorer.tracksOf(player)
        val ask = JSONObject()
            .put("mode", LANE)
            .put("seed", JSONArray(listOf(playing.optString("id"))))
            .put("exclude", JSONArray(queue.map { it.optString("id") }))
            .put("last", playing.optString("id"))
            .put("count", COUNT)
        val app = context.applicationContext
        asking = key
        Scorer.asks.execute {
            val artists = setOfNotNull(playing.optString("artist").takeIf { it.isNotEmpty() })
            val got = server?.let { Scorer.ask(it, ask) } ?: Scorer.offline(app, queue, artists, false, COUNT, LANE)
            Scorer.main.post {
                asking = null
                // Still on, still that track, still without its inserts.
                if (!on(app) || got.length() == 0) return@post
                val now = player.currentMediaItem?.let(::trackOf)?.optString("entry")?.ifEmpty { null } ?: "${player.currentMediaItemIndex}"
                if (now != key) return@post
                val at = placeFor(player) ?: return@post
                player.addMediaItems(at, mediaItems(got).map(locate))
            }
        }
    }

    /** Where the playing track's inserts go: after it and its run of tracks added by hand; null when they are there. */
    private fun placeFor(player: Player): Int? {
        val count = player.mediaItemCount
        var at = player.currentMediaItemIndex + 1
        while (at < count && trackOf(player.getMediaItemAt(at))?.optString("lane") == "next") at++
        return if (at < count && trackOf(player.getMediaItemAt(at))?.optString("lane") == LANE) null else at
    }
}

object Autoplay {
    const val LANE = "autoplay"
    private const val BATCH = 5
    private const val LOW = 3

    /** On with its mode (similar, artist, era, random), whole albums, restarting playlists, its seed (empty: the
     *  queue's own tracks) and a crate as its bound; null: off ("stop"). */
    data class State(val mode: String, val albums: Boolean, val restart: Boolean, val seed: List<String>, val within: String?)

    @Volatile private var filling = false

    fun get(context: Context): State? = Prefs(context).autoplay

    /** On, as `state`, or off (null: its tracks to come go). On turns repeat all off. */
    fun set(context: Context, player: Player, state: State?) {
        Prefs(context).autoplay = state
        if (state == null) {
            Scorer.dropAhead(player, LANE)
            return
        }
        if (Repeat.mode(context) == Repeat.ALL) Repeat.set(context, player, Repeat.OFF) { it }
    }

    /** An `autoplay` op or command: `mode` ("stop": off), `albums`, `restart`, `seed`, `within` ("" or null: out of the
     *  crate); what is left out stays. Another mode drops what the old one chose. */
    fun command(context: Context, player: Player, m: JSONObject) {
        val mode = m.optString("mode").takeIf { it.isNotEmpty() && it != "null" }
        if (mode == "stop") return set(context, player, null)
        val was = get(context)
        val seed = m.optJSONArray("seed")?.let { a -> (0 until a.length()).map { a.optString(it) }.filter { it.isNotEmpty() } }
        val within = when {
            !m.has("within") -> was?.within
            m.isNull("within") -> null
            else -> m.optString("within").ifEmpty { null }
        }
        val next = State(
            mode ?: was?.mode ?: "similar",
            if (m.has("albums")) m.optBoolean("albums") else was?.albums ?: false,
            if (m.has("restart")) m.optBoolean("restart") else was?.restart ?: false,
            seed ?: was?.seed ?: emptyList(),
            within,
        )
        if (was != null && (next.mode != was.mode || next.albums != was.albums || next.within != was.within)) Scorer.dropAhead(player, LANE)
        set(context, player, next)
    }

    /** Something new plays (`tracks`): Autoplay stays on, its seed and bound cleared unless every track is its own
     *  ("Start radio" plays its first tracks in its lane). */
    fun newPlay(context: Context, tracks: JSONArray?) {
        val a = get(context) ?: return
        val all = tracks != null && tracks.length() > 0 && (0 until tracks.length()).all { tracks.optJSONObject(it)?.optString("lane") == LANE }
        if (!all && (a.seed.isNotEmpty() || a.within != null)) Prefs(context).autoplay = a.copy(seed = emptyList(), within = null)
    }

    /** For a report: `{mode, albums, restart, seed, within}`, or null while off. */
    fun json(context: Context): Any {
        val a = get(context) ?: return JSONObject.NULL
        return JSONObject().put("mode", a.mode).put("albums", a.albums).put("restart", a.restart).put("seed", JSONArray(a.seed)).put("within", a.within ?: JSONObject.NULL)
    }

    /** With Autoplay on and fewer than three tracks ahead: the queue's own tracks again (Restart, once they have all
     *  played), else the next ones by its mode, appended. Called on the main thread. */
    fun fill(context: Context, player: Player, server: String?, locate: (MediaItem) -> MediaItem) {
        val a = get(context) ?: return
        val count = player.mediaItemCount
        val c = player.currentMediaItemIndex
        if (filling || count == 0 || count - c - 1 >= LOW) return
        val queue = Scorer.tracksOf(player)
        val own = queue.filter { it.optString("lane").isEmpty() }
        if (a.restart) {
            // Not before the last of them plays.
            if ((c + 1 until count).any { trackOf(player.getMediaItemAt(it))?.optString("lane").isNullOrEmpty() }) return
            restart(context, player, own, queue.getOrNull(c), locate)
            return
        }
        val playing = queue.getOrNull(c)
        val ask = JSONObject()
            .put("mode", a.mode)
            .put("albums", a.albums)
            .put("seed", JSONArray(a.seed.ifEmpty { own.map { it.optString("id") } }))
            .put("within", a.within ?: JSONObject.NULL)
            .put("exclude", JSONArray(queue.map { it.optString("id") }))
            .put("last", playing?.optString("id") ?: JSONObject.NULL)
            .put("count", BATCH)
        val app = context.applicationContext
        filling = true
        Scorer.asks.execute {
            val artists = buildSet {
                playing?.optString("artist")?.takeIf { it.isNotEmpty() }?.let(::add)
                for (t in queue) if (t.optString("id") in a.seed) t.optString("artist").takeIf { it.isNotEmpty() }?.let(::add)
            }
            val got = server?.let { Scorer.ask(it, ask) } ?: Scorer.offline(app, queue, artists, a.mode == "random", BATCH, LANE)
            Scorer.main.post {
                filling = false
                if (get(app) == null || got.length() == 0) return@post
                player.addMediaItems(mediaItems(got).map(locate))
            }
        }
    }

    /** Restart: the queue's own tracks again, as own tracks (radio inserts again too): in their order, or shuffled when
     *  the queue plays shuffled, never starting with the one that just played. */
    private fun restart(context: Context, player: Player, own: List<JSONObject>, last: JSONObject?, locate: (MediaItem) -> MediaItem) {
        val once = own.distinctBy { if (it.has("n")) "n${it.optInt("n")}" else "${it.optString("source")}/${it.optString("id")}" }
        if (once.isEmpty()) return
        val round = if (Prefs(context).shuffled) {
            once.shuffled().toMutableList().also { r ->
                if (r.size > 1 && last != null && r[0].optString("id") == last.optString("id")) r.add(r.removeAt(0))
            }
        } else {
            once.sortedBy { if (it.has("n")) it.optInt("n") else Int.MAX_VALUE }
        }
        player.addMediaItems(mediaItems(JSONArray(round.map { JSONObject(it.toString()).apply { remove("entry") } })).map(locate))
    }
}
