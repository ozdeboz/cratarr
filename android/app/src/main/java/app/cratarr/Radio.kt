/*
 * Cratarr's own (decision 0036, amended: Radio and Autoplay apart, as Symfonium's). Both are kept by this device's
 * player, which alone knows when a track starts and how many are left, offline too.
 * - Radio inserts: while on, each of the queue's own tracks (no lane) that starts playing gets about five tracks like it
 *   right after it and its run of tracks added by hand, in the "radio" lane; off drops those not yet played.
 * - Autoplay appends: when fewer than three tracks are left ahead, the next five by its mode (lane "autoplay"). Stop
 *   drops those not yet played. (Its Restart, the queue's own tracks again, is no more: the owner, 2026-10-24.)
 * Cratarr's scorer chooses (`POST /api/radio/next`); without a connection this device answers for itself, by the same rules, among
 * what plays here ([DeviceAnswers]), with no notice of what cannot. With nothing kept to answer from, its downloads. When to ask and
 * where what comes goes are the rules crate's too: Kotlin holds no decisions. Autoplay and repeat all exclude each other; radio lives
 * with both.
 */
package app.cratarr

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import app.cratarr.rules.KeptPick
import app.cratarr.rules.QueueLane
import app.cratarr.rules.autoplayBatch
import app.cratarr.rules.autoplayDue
import app.cratarr.rules.picksFromKept
import app.cratarr.rules.radioPlace
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors
import kotlin.random.Random

/** A queue's lanes as the rules crate names them. */
internal fun laneOf(track: JSONObject?): QueueLane = when (track?.optString("lane")) {
    "next" -> QueueLane.NEXT
    "radio" -> QueueLane.RADIO
    "autoplay" -> QueueLane.AUTOPLAY
    else -> QueueLane.OWN
}

/** What both ask of Cratarr, and how they choose without it. */
private object Scorer {
    const val TAG = "CratarrRadio"
    val asks = Executors.newSingleThreadExecutor()
    val main = Handler(Looper.getMainLooper())

    /** Cratarr's choice; null without an answer (offline). */
    fun ask(context: Context, server: String, body: JSONObject): JSONArray? = try {
        val c = Net.open(context, "${server.trimEnd('/')}/api/radio/next")
        c.requestMethod = "POST"
        c.connectTimeout = 5_000
        c.readTimeout = 15_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
        val ok = Net.code(context, c) in 200..299
        val text = if (ok) c.inputStream.bufferedReader().use { it.readText() } else null
        c.disconnect()
        text?.let { JSONObject(it).optJSONArray("tracks") }
    } catch (e: Exception) {
        Log.w(TAG, "radio: ${e.message}")
        null
    }

    /** Cratarr's own library, the one a device holds. */
    private const val CRATARR = "cratarr"

    private fun keyOf(track: JSONObject) = "${track.optString("source")}/${track.optString("id")}"

    /** Without a connection: what this device answers for `ask` among what plays here; with nothing kept to answer from, of the downloads
     *  not in the queue, `artists` first (`any`: in any order), at most `count`, in `lane`. No rules library: nothing. */
    fun offline(context: Context, ask: JSONObject, queue: List<JSONObject>, artists: Set<String>, any: Boolean, count: Int, lane: String): JSONArray {
        val downloaded = Downloads.list(context).optJSONArray("tracks") ?: JSONArray()
        val kept = (0 until downloaded.length())
            .mapNotNull { downloaded.optJSONObject(it) }
            .filter { it.optJSONObject("download")?.optString("state") == "done" }
        return try {
            val here = kept.filter { it.optString("source") == CRATARR }.map { it.optString("id") }
            DeviceAnswers.pick(context, ask, here) ?: run {
                val picks = picksFromKept(kept.map { KeptPick(keyOf(it), it.optString("artist").ifEmpty { null }) }, queue.map(::keyOf), artists.toList(), any, count.toUInt(), Random.nextLong(Long.MAX_VALUE shr 11).toULong())
                JSONArray(picks.map { JSONObject(kept[it.toInt()].toString()).apply { remove("download"); remove("added"); remove("entry"); remove("n"); remove("asked_by"); put("lane", lane) } })
            }
        } catch (e: LinkageError) {
            Log.w(TAG, "radio: no rules library: $e")
            JSONArray()
        }
    }

    fun tracksOf(player: Player) = (0 until player.mediaItemCount).mapNotNull { trackOf(player.getMediaItemAt(it)) }

    /** Drops the tracks of `lane` still to come. */
    fun dropAhead(player: Player, lane: String) {
        for (i in player.mediaItemCount - 1 downTo player.currentMediaItemIndex + 1) {
            if (trackOf(player.getMediaItemAt(i))?.optString("lane") == lane) player.removeMediaItem(i)
        }
    }
}

/**
 * Ignore favourites (decision 0036, amended; the owner's): Radio's inserts and Autoplay's picks leave out every track the
 * owner hearted (Cratarr's scorer does, told with each ask; without a connection the downloads are chosen from as they
 * are). Changed, both lanes' tracks still to come go, and both choose anew: the prefs' change tells the service.
 */
object SkipFavourites {
    fun on(context: Context): Boolean = Prefs(context).skipFavourites

    fun set(context: Context, player: Player, on: Boolean) {
        val prefs = Prefs(context)
        if (prefs.skipFavourites == on) return
        Scorer.dropAhead(player, Radio.LANE)
        Scorer.dropAhead(player, Autoplay.LANE)
        Radio.forget()
        prefs.skipFavourites = on
    }
}

object Radio {
    const val LANE = "radio"
    /** Inserts after each own track unless told; at most [MAX]. */
    const val COUNT = 5
    const val MAX = 10
    /** What its tracks are like: the track (the first, the default), its artist, its era. */
    val BYS = listOf("track", "artist", "era")

    /** The playing track's entry asked for, while it is being asked: one ask per track. */
    @Volatile private var asking: String? = null

    fun on(context: Context): Boolean = Prefs(context).radioOn

    /** On or off, with what its tracks are like and how many (left out: as they were). Another choice while on drops
     *  the inserts still to come; the playing track gets new ones (the prefs' change says so to the service). */
    fun set(context: Context, player: Player, on: Boolean, by: String? = null, count: Int? = null) {
        val prefs = Prefs(context)
        val was = Triple(prefs.radioOn, prefs.radioBy, prefs.radioCount)
        by?.takeIf { it in BYS }?.let { prefs.radioBy = it }
        count?.let { prefs.radioCount = it }
        prefs.radioOn = on
        val other = (was.second to was.third) != (prefs.radioBy to prefs.radioCount)
        if (!on || (was.first && other)) {
            Scorer.dropAhead(player, LANE)
            asking = null
        }
    }

    /** The playing track asked for no more: it gets its inserts anew. */
    fun forget() {
        asking = null
    }

    /** As reports and the page say it. */
    fun by(context: Context): String = Prefs(context).radioBy

    fun count(context: Context): Int = Prefs(context).radioCount

    /** A `radio` op or command. One with a mode, seed or bound is the older shape: Autoplay (decision 0036, amended);
     *  else Radio alone (off leaves Autoplay be). */
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
        val count = if (m.has("count") && !m.isNull("count")) m.optInt("count") else null
        set(context, player, m.optBoolean("on"), m.optString("by").takeIf { it in BYS }, count)
    }

    /** With radio on, the playing track one of the queue's own: tracks like it after it and its hand-added run, unless
     *  they are there already (a seek, a replay, a report). Called on the main thread. */
    fun insert(context: Context, player: Player, server: String?, locate: (MediaItem) -> MediaItem) {
        // Not for the device's own music (decision 0052); a server's mode (decision 0098) has that server's own mix.
        val mode = Prefs(context).mode
        if (!on(context) || mode == Prefs.MODE_DEVICE) return
        val mix = mode.takeIf { it.startsWith(Servers.MODE_PREFIX) }
        val c = player.currentMediaItemIndex
        val count = player.mediaItemCount
        if (c < 0 || c >= count) return
        val playing = trackOf(player.getMediaItemAt(c)) ?: return
        val key = playing.optString("entry").ifEmpty { "$c" }
        if (asking == key || placeFor(player) == null) return
        val queue = Scorer.tracksOf(player)
        val (by, n) = by(context) to count(context)
        val skip = SkipFavourites.on(context)
        val ask = JSONObject()
            .put("mode", LANE)
            .put("by", by)
            .put("skip_favourites", skip)
            .put("seed", JSONArray(listOf(playing.optString("id"))))
            .put("exclude", JSONArray(queue.map { it.optString("id") }))
            .put("last", playing.optString("id"))
            .put("count", n)
        val app = context.applicationContext
        asking = key
        Scorer.asks.execute {
            val artists = setOfNotNull(playing.optString("artist").takeIf { it.isNotEmpty() })
            val got = if (mix != null) {
                Servers.mix(app, mix, playing.optString("id"), n, queue.map { it.optString("id") }.toSet(), LANE)
            } else {
                server?.let { Scorer.ask(app, it, ask) } ?: Scorer.offline(app, ask, queue, artists, false, n, LANE)
            }
            Scorer.main.post {
                if (asking == key) asking = null
                // Still on as asked, still that track, still without its inserts.
                if (!on(app) || by(app) != by || count(app) != n || SkipFavourites.on(app) != skip || got.length() == 0) return@post
                val now = player.currentMediaItem?.let(::trackOf)?.optString("entry")?.ifEmpty { null } ?: "${player.currentMediaItemIndex}"
                if (now != key) return@post
                val at = placeFor(player) ?: return@post
                player.addMediaItems(at, mediaItems(got).map(locate))
            }
        }
    }

    /** Where the playing track's inserts go; null where none do. */
    private fun placeFor(player: Player): Int? = try {
        radioPlace((0 until player.mediaItemCount).map { laneOf(trackOf(player.getMediaItemAt(it))) }, player.currentMediaItemIndex.toUInt())?.toInt()
    } catch (e: LinkageError) {
        null
    }
}

object Autoplay {
    const val LANE = "autoplay"

    /** On with its mode (similar, artist, era, random, taste), whole albums, restarting playlists, its seed (empty: the
     *  queue's own tracks) and what it stays inside (decision 0094: the server's `{kind, …}`, kept as its JSON text);
     *  null: off ("stop"). */
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

    /** An `autoplay` op or command: `mode` ("stop": off), `albums`, `restart`, `seed`, `within` (null: beyond what it
     *  stayed inside); what is left out stays. Another mode drops what the old one chose. */
    fun command(context: Context, player: Player, m: JSONObject) {
        val mode = m.optString("mode").takeIf { it.isNotEmpty() && it != "null" }
        if (mode == "stop") return set(context, player, null)
        val was = get(context)
        val seed = m.optJSONArray("seed")?.let { a -> (0 until a.length()).map { a.optString(it) }.filter { it.isNotEmpty() } }
        val within = when {
            !m.has("within") -> was?.within
            m.isNull("within") -> null
            else -> m.optJSONObject("within")?.toString()
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
        return JSONObject().put("mode", a.mode).put("albums", a.albums).put("restart", a.restart).put("seed", JSONArray(a.seed)).put("within", a.within?.let { JSONObject(it) } ?: JSONObject.NULL)
    }

    /** With Autoplay on and fewer than three tracks ahead: the next ones by its mode, appended. Called on the main
     *  thread. */
    fun fill(context: Context, player: Player, server: String?, locate: (MediaItem) -> MediaItem) {
        val a = get(context) ?: return
        // Not for the device's own music; a server's mode has that server's own mix, from the track that plays.
        val mode = Prefs(context).mode
        if (mode == Prefs.MODE_DEVICE) return
        val mix = mode.takeIf { it.startsWith(Servers.MODE_PREFIX) }
        val count = player.mediaItemCount
        val c = player.currentMediaItemIndex
        if (filling || c < 0) return
        val batch = try {
            if (!autoplayDue(count.toUInt(), c.toUInt())) return
            autoplayBatch().toInt()
        } catch (e: LinkageError) {
            return
        }
        val queue = Scorer.tracksOf(player)
        val own = queue.filter { it.optString("lane").isEmpty() }
        val playing = queue.getOrNull(c)
        val skip = SkipFavourites.on(context)
        val ask = JSONObject()
            .put("mode", a.mode)
            .put("skip_favourites", skip)
            .put("albums", a.albums)
            .put("seed", JSONArray(a.seed.ifEmpty { own.map { it.optString("id") } }))
            .put("within", a.within?.let { JSONObject(it) } ?: JSONObject.NULL)
            .put("exclude", JSONArray(queue.map { it.optString("id") }))
            .put("last", playing?.optString("id") ?: JSONObject.NULL)
            .put("count", batch)
        val app = context.applicationContext
        val who = Who.current(app)
        filling = true
        Scorer.asks.execute {
            val artists = buildSet {
                playing?.optString("artist")?.takeIf { it.isNotEmpty() }?.let(::add)
                for (t in queue) if (t.optString("id") in a.seed) t.optString("artist").takeIf { it.isNotEmpty() }?.let(::add)
            }
            val got = if (mix != null) {
                val from = playing?.optString("id")?.ifEmpty { null } ?: own.lastOrNull()?.optString("id")
                from?.let { Servers.mix(app, mix, it, batch, queue.map { t -> t.optString("id") }.toSet(), LANE) } ?: JSONArray()
            } else {
                server?.let { Scorer.ask(app, it, ask) } ?: Scorer.offline(app, ask, queue, artists, a.mode == "random", batch, LANE)
            }
            Scorer.main.post {
                filling = false
                // Asked for another's queue: its own account's asks for itself.
                if (Who.current(app) != who) return@post
                // Off meanwhile, or asked before favourites were ignored (or no longer): what came is not wanted.
                if (get(app) == null || SkipFavourites.on(app) != skip || got.length() == 0) {
                    if (SkipFavourites.on(app) != skip) fill(app, player, server, locate)
                    return@post
                }
                player.addMediaItems(mediaItems(got).map(locate))
            }
        }
    }
}
