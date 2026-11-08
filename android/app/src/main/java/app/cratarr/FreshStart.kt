/*
 * The one fresh start of 0.16 (decision 0101 point 23; the owner, 2026-11-08: "Build it into the app"): every id moved at the
 * server's first start, so what this device keeps under the old ones goes once, and what each account had downloaded and kept
 * comes again under the new. What to do is the rules crate's (`fresh`, through UniFFI); this keeps the list and does it.
 * - Collect: each account's own downloads (their tracks as they were asked for) and its kept items, into one file, written whole
 *   or not at all, and never again once it is there: a second start goes on from it, never from a half-cleared device.
 * - Clear: the kept lists, every download and its file, and the library kept on the device. Kept: the address, the sign-ins, whose
 *   things show, the settings, unsent plays and the saved queues (Cratarr follows their old ids: a queue plays on).
 * - Mark: the last step, so a stop anywhere before it does the rest again.
 * - Fetch: for the account signed in alone (point 28), once Cratarr answers and the downloads' own network setting allows: the
 *   ids it has now asked of Cratarr (`/api/moved`, `/api/playlists/moved`), its downloads asked again and its items kept again,
 *   then its part of the list goes. Another's waits for its own sign-in. Never while the kept lists cannot be read.
 * A device with no rules library takes none: it keeps no index either, and Cratarr still serves its files by their old ids.
 * All of it runs on one thread.
 */
package app.cratarr

import android.content.Context
import android.util.Log
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.DownloadService
import app.cratarr.rules.FreshFacts
import app.cratarr.rules.FreshStep
import app.cratarr.rules.freshStep
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.util.concurrent.Executors

object FreshStart {
    private const val TAG = "CratarrFresh"
    private const val FILE = "fresh-0.16.json"
    private const val MARK = "fresh_start_done"

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "cratarr-fresh").apply { isDaemon = true } }

    /** Looks, and does what is due: at the app's and the service's start, at a sign-in, on a new network. */
    fun look(context: Context) {
        val app = context.applicationContext
        worker.execute {
            try {
                FreshLooks(RustFreshRule, AndroidFresh(app), File(app.filesDir, FILE), BuildConfig.VERSION_CODE).look()
            } catch (e: LinkageError) {
                Log.w(TAG, "no rules library: $e")
            } catch (e: Exception) {
                Log.w(TAG, "a look: $e")
            }
        }
    }

    /** The device as the looks see it. */
    @androidx.annotation.OptIn(UnstableApi::class)
    private class AndroidFresh(private val app: Context) : FreshDevice {
        private val prefs get() = Prefs.shared(app)

        override fun marked(): Int? = prefs.getInt(MARK, -1).takeIf { it >= 0 }

        override fun mark(version: Int) {
            prefs.edit().putInt(MARK, version).commit()
        }

        override fun snapshot() = Kept.snapshot(app)

        override fun downloads(): Map<String, JSONObject> {
            val out = mutableMapOf<String, JSONObject>()
            Downloads.manager(app).downloadIndex.getDownloads().use { c ->
                while (c.moveToNext()) {
                    val d = c.download
                    out[d.request.id] = try {
                        JSONObject(String(d.request.data))
                    } catch (e: JSONException) {
                        JSONObject()
                    }
                }
            }
            return out
        }

        override fun clear(): Boolean {
            if (!Kept.forgetAll(app)) return false
            DownloadService.sendRemoveAllDownloads(app, CratarrDownloadService::class.java, false)
            // Gone from the index means gone with its file: the download service takes both.
            val until = System.currentTimeMillis() + 60_000
            while (downloads().isNotEmpty()) {
                if (System.currentTimeMillis() > until) return false
                Thread.sleep(250)
            }
            DeviceIndex.forget(app)
            return true
        }

        override fun who() = Who.current(app)

        override fun reached() = Prefs(app).serverUrl != null && NetState.connected(app) && !Net.refused(app, Prefs(app).serverUrl!!)

        override fun mayDownload() = Prefs(app).downloadOverMobile || !Net.metered(app)

        override fun moved(ids: List<String>): Map<String, String>? {
            val out = mutableMapOf<String, String>()
            for (some in ids.chunked(MOVED_AT_ONCE)) {
                val o = asked("/api/moved", JSONObject().put("ids", JSONArray(some))) ?: return null
                for (k in o.keys()) out[k] = o.getString(k)
            }
            return out
        }

        override fun playlistsMoved(): Map<String, String>? = asked("/api/playlists/moved", null)?.let { o -> o.keys().asSequence().associateWith { o.getString(it) } }

        /** Cratarr's answer as JSON; null for none, or one that was no answer to this. */
        private fun asked(path: String, body: JSONObject?): JSONObject? = try {
            val server = Prefs(app).serverUrl?.trimEnd('/') ?: return null
            val c = Net.open(app, server + path)
            c.connectTimeout = 10_000
            c.readTimeout = 30_000
            if (body != null) {
                c.requestMethod = "POST"
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            }
            val ok = Net.code(app, c) in 200..299
            val text = if (ok) c.inputStream.bufferedReader().use { it.readText() } else null
            c.disconnect()
            text?.let { JSONObject(it) }
        } catch (e: IOException) {
            null
        } catch (e: JSONException) {
            null
        }

        override fun download(tracks: JSONArray) = Downloads.add(app, tracks)

        override fun keep(kind: String, source: String, id: String, name: String) = Kept.set(app, kind, source, id, name, true)
    }

    /** As many ids as Cratarr takes in one ask. */
    private const val MOVED_AT_ONCE = 500
}

/** What a look asks of the device and does to it: Android's in the app, a stand-in in the tests. */
internal interface FreshDevice {
    fun marked(): Int?
    fun mark(version: Int)
    /** The kept items of every account and each download's holders; null while the lists cannot be read. */
    fun snapshot(): Pair<List<JSONObject>, Map<String, Set<String>>>?
    /** Every download's track, as asked for, by its id. */
    fun downloads(): Map<String, JSONObject>
    /** The stores of before gone; false where it could not be done whole, to be done again. */
    fun clear(): Boolean
    fun who(): String
    fun reached(): Boolean
    fun mayDownload(): Boolean
    /** What these ids are now, those that moved; null with no answer. */
    fun moved(ids: List<String>): Map<String, String>?
    fun playlistsMoved(): Map<String, String>?
    fun download(tracks: JSONArray)
    fun keep(kind: String, source: String, id: String, name: String)
}

/** The rule, as the rules crate says it; in the unit tests, which have no library, a stand-in. */
internal interface FreshRule {
    fun step(facts: FreshFacts, version: Int): FreshStep
}

internal object RustFreshRule : FreshRule {
    override fun step(facts: FreshFacts, version: Int) = freshStep(facts, version.toUInt())
}

/** The looks themselves, with nothing of Android's in them. [file] holds what waits to be fetched again. */
internal class FreshLooks(private val rule: FreshRule, private val device: FreshDevice, private val file: File, private val version: Int) {

    /** What is due, done, step by step, as far as it goes now. */
    fun look() {
        repeat(STEPS_AT_MOST) {
            val held = read()
            val readable = device.snapshot() != null
            val facts = FreshFacts(
                marked = device.marked()?.toUInt(),
                collected = held != null,
                cleared = held?.optBoolean("cleared") == true,
                keptReadable = readable,
                waitingHere = held?.optJSONObject("accounts")?.has(device.who()) == true,
                reached = device.reached(),
                mayDownload = device.mayDownload(),
            )
            when (rule.step(facts, version)) {
                FreshStep.NOTHING -> return
                FreshStep.COLLECT -> collect() ?: return
                FreshStep.CLEAR -> if (device.clear()) write(held!!.put("cleared", true)) else return
                FreshStep.MARK -> device.mark(version)
                FreshStep.FETCH -> if (!fetch(held!!)) return
            }
        }
    }

    /** Each account's own downloads and kept items, written down; null where the lists could not be read. */
    private fun collect(): Unit? {
        val (items, holders) = device.snapshot() ?: return null
        val downloads = device.downloads()
        val accounts = JSONObject()
        fun of(who: String) = accounts.optJSONObject(who) ?: JSONObject().put("downloads", JSONArray()).put("kept", JSONArray()).also { accounts.put(who, it) }
        for ((id, held) in holders) {
            val track = downloads[id] ?: continue
            for (who in held.filter(KeptHolds::isManual).map(KeptHolds::whoOf).toSortedSet()) {
                of(who).getJSONArray("downloads").put(JSONObject(track.toString()).apply { remove("added") })
            }
        }
        for (item in items) {
            of(item.getString("who")).getJSONArray("kept").put(JSONObject(item.toString()).apply { remove("who") })
        }
        write(JSONObject().put("cleared", false).put("accounts", accounts))
        return Unit
    }

    /** What the account signed in had, asked again under the ids it has now: whether it was done. */
    private fun fetch(held: JSONObject): Boolean {
        val who = device.who()
        val mine = held.getJSONObject("accounts").getJSONObject(who)
        val downloads = mine.getJSONArray("downloads")
        val kept = mine.getJSONArray("kept")
        val ids = mutableSetOf<String>()
        for (t in downloads.objects()) if (t.optString("source") == CRATARR) listOf("id", "album_id").forEach { f -> t.optString(f).takeIf(String::isNotEmpty)?.let(ids::add) }
        for (k in kept.objects()) if (k.optString("source") == CRATARR && k.optString("kind") != PLAYLIST) ids.add(k.optString("id"))
        val moved = if (ids.isEmpty()) emptyMap() else device.moved(ids.sorted()) ?: return false
        val lists = if (kept.objects().any { it.optString("kind") == PLAYLIST }) device.playlistsMoved() ?: return false else emptyMap()
        // Another came meanwhile: what is fetched is the account's that was signed in, and goes to it alone.
        if (device.who() != who) return false
        val now = JSONArray(
            downloads.objects().map { t ->
                JSONObject(t.toString()).apply {
                    if (t.optString("source") == CRATARR) listOf("id", "album_id").forEach { f -> moved[t.optString(f)]?.let { put(f, it) } }
                }
            },
        )
        if (now.length() > 0) device.download(now)
        for (k in kept.objects()) {
            val id = k.optString("id")
            val to = if (k.optString("kind") == PLAYLIST) lists[id] else moved[id].takeIf { k.optString("source") == CRATARR }
            device.keep(k.optString("kind"), k.optString("source"), to ?: id, k.optString("name"))
        }
        val accounts = held.getJSONObject("accounts")
        accounts.remove(who)
        write(held)
        return true
    }

    /** The list; null where none was written. One there that cannot be read (it is written whole, so only a damaged disk does that)
     *  is one with nothing left in it, never none: writing it down again now would take a half-cleared device for what it had. */
    private fun read(): JSONObject? = if (!file.exists()) null else try {
        JSONObject(file.readText())
    } catch (e: JSONException) {
        JSONObject().put("cleared", true).put("accounts", JSONObject())
    }

    private fun write(o: JSONObject) = KeptFile.write(file, o.toString())

    private fun JSONArray.objects(): List<JSONObject> = (0 until length()).mapNotNull { optJSONObject(it) }

    private companion object {
        const val CRATARR = "cratarr"
        const val PLAYLIST = "playlist"
        /** Collect, clear, mark, fetch, and a look to say nothing is left. */
        const val STEPS_AT_MOST = 6
    }
}
