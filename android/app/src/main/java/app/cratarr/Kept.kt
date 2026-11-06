/*
 * Kept on this device (decision 0038): albums, playlists, crates and artists the app keeps in step offline. A kept
 * item is followed: Cratarr says what it holds now (POST /api/kept, with the version last seen, so an unchanged item
 * costs a hash), its new tracks are downloaded and the ones that left it are removed, through the same download store
 * as the owner's own downloads (Downloads).
 * - A track goes only when nothing holds it: each download has its holders, each an account's ([Who], decision 0101):
 *   its `manual` (its Download) and its kept items' keys. A file two accounts hold is stored once; an account sees only
 *   what it holds, and its items are asked after only while it is signed in (the others keep what they have).
 * - One storage budget for the device (20 GB unless set; 0 is no limit). Every account's downloads count first and
 *   are never dropped; then the items, the newest kept first, each in its own order: what does not fit is left out,
 *   so the item kept longest ago shrinks first.
 * - Synced when the app starts, when an item is kept, when the settings change, and every few hours by WorkManager
 *   (KeptWorker), with the downloads' own rules: Wi-Fi unless mobile data is allowed, charging when asked.
 * - The lists are written whole (KeptFile). A list of the items that is there and cannot be read stops the sync and any
 *   keep, and what it held stays on the device: read as nothing kept, the next sync would remove it all.
 * All of it runs on one thread, so the lists are never changed at once from two places.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadRequest
import androidx.media3.exoplayer.offline.DownloadService
import androidx.media3.exoplayer.scheduler.Requirements
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

@OptIn(UnstableApi::class)
object Kept {
    private const val TAG = "CratarrKept"
    private const val ITEMS_FILE = "kept-items.json"
    private const val HOLDERS_FILE = "kept-holders.json"
    const val MANUAL = "manual"
    /** A track whose size Cratarr does not know counts as this much against the budget. */
    private const val UNKNOWN_SIZE = 10_000_000L
    private const val WORK = "cratarr-kept"

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "cratarr-kept").apply { isDaemon = true } }

    /** Told after anything changed (MainActivity sends the page a report). */
    @Volatile var onChange: (() -> Unit)? = null

    /** Sync passes going on now (the report's `running`). */
    private val passes = java.util.concurrent.atomic.AtomicInteger(0)

    /** A holder of a download: [Who]'s key and what holds it ([MANUAL] or an item's key), apart by a tab. */
    private fun held(who: String, what: String) = "$who\t$what"
    private fun whoOf(holder: String) = holder.substringBefore('\t')
    private fun isManual(holder: String) = holder.substringAfter('\t') == MANUAL

    /** A kept item, as stored: whose it is, its tracks as Cratarr last said, and how many the budget left out. */
    private class Item(
        val who: String,
        val kind: String,
        val source: String,
        val id: String,
        var name: String,
        val since: Long,
        var version: String?,
        var tracks: JSONArray,
        var error: String?,
        var overBudget: Int,
    ) {
        val key get() = "$kind:$source/$id"
        /** Its hold on a download. */
        val holder get() = "$who\t$key"

        fun json(): JSONObject = JSONObject()
            .put("who", who).put("kind", kind).put("source", source).put("id", id).put("name", name).put("since", since)
            .put("version", version ?: JSONObject.NULL).put("tracks", tracks).put("error", error ?: JSONObject.NULL)
            .put("overBudget", overBudget)

        companion object {
            fun of(o: JSONObject) = Item(
                o.optString("who"),
                o.optString("kind"), o.optString("source"), o.optString("id"), o.optString("name"), o.optLong("since"),
                o.optString("version").takeIf { o.has("version") && !o.isNull("version") && it.isNotEmpty() },
                o.optJSONArray("tracks") ?: JSONArray(),
                o.optString("error").takeIf { o.has("error") && !o.isNull("error") && it.isNotEmpty() },
                o.optInt("overBudget"),
            )
        }
    }

    // --- the stored lists (on the worker thread only) ---

    private fun file(context: Context, name: String) = File(context.applicationContext.filesDir, name)

    /** The kept items; throws when the list cannot be read ([KeptFile.list]), so nothing goes on from a list that is not it. */
    private fun loadItems(context: Context): MutableList<Item> =
        KeptFile.list(file(context, ITEMS_FILE)).map { Item.of(it) }.filter { it.who.isNotEmpty() }.toMutableList()

    /** The kept items to show: none while the list cannot be read. Only what changes things has to stop. */
    private fun shownItems(context: Context): List<Item> = try {
        loadItems(context)
    } catch (e: IOException) {
        Log.w(TAG, "the list: ${e.message}")
        emptyList()
    }

    private fun saveItems(context: Context, items: List<Item>) {
        KeptFile.write(file(context, ITEMS_FILE), JSONArray(items.map { it.json() }).toString())
    }

    /** Each download's holders ([held]), by its id (`source/id`). */
    private fun loadHolders(context: Context): MutableMap<String, MutableSet<String>> = try {
        val o = JSONObject(file(context, HOLDERS_FILE).readText())
        o.keys().asSequence().associateWith { k ->
            val a = o.getJSONArray(k)
            (0 until a.length()).map { a.getString(it) }.filter { '\t' in it }.toMutableSet()
        }.toMutableMap()
    } catch (e: Exception) {
        mutableMapOf()
    }

    private fun saveHolders(context: Context, holders: Map<String, Set<String>>) {
        val o = JSONObject()
        for ((k, v) in holders) if (v.isNotEmpty()) o.put(k, JSONArray(v.toList()))
        KeptFile.write(file(context, HOLDERS_FILE), o.toString())
    }

    // --- what the page and the other parts call ---

    /** Keep an item, or not, for the account signed in; then sync. */
    fun set(context: Context, kind: String, source: String, id: String, name: String, on: Boolean) {
        if (on && source == Previews.SOURCE) return
        val app = context.applicationContext
        worker.execute {
            try {
                val who = Who.current(app)
                val items = loadItems(app)
                val key = "$kind:$source/$id"
                items.removeAll { it.who == who && it.key == key }
                if (on) items.add(Item(who, kind, source, id, name.ifEmpty { id }, System.currentTimeMillis(), null, JSONArray(), null, 0))
                saveItems(app, items)
            } catch (e: IOException) {
                // A list that cannot be read is not written over, and a full disk is no reason to end the app and its player.
                Log.w(TAG, "keep: ${e.message}")
                return@execute
            }
            syncHere(app, retryFailed = false)
        }
    }

    /** Sync now (on the worker thread); `retryFailed` starts failed downloads again (the sheet's retry). */
    fun sync(context: Context, retryFailed: Boolean = false) {
        val app = context.applicationContext
        worker.execute { syncHere(app, retryFailed) }
    }

    /** Sync and wait (WorkManager's thread). */
    fun syncAndWait(context: Context) {
        val app = context.applicationContext
        worker.submit { syncHere(app, retryFailed = false) }.get(10, TimeUnit.MINUTES)
    }

    /** The account's Download: these tracks are held by its `manual` from now on (Downloads.add calls it). */
    fun holdManual(context: Context, keys: List<String>) {
        val app = context.applicationContext
        worker.execute {
            val mine = held(Who.current(app), MANUAL)
            val holders = loadHolders(app)
            for (k in keys) holders.getOrPut(k) { mutableSetOf() }.add(mine)
            try {
                saveHolders(app, holders)
            } catch (e: IOException) {
                Log.w(TAG, "hold: ${e.message}")
            }
        }
    }

    /**
     * The account signed in removes downloads (all of its own with `all`): its `manual` hold goes; a track one of its
     * kept items or another account still holds stays, any other is removed.
     */
    fun releaseManual(context: Context, keys: List<String>, all: Boolean) {
        val app = context.applicationContext
        worker.execute {
            val mine = held(Who.current(app), MANUAL)
            val holders = loadHolders(app)
            val targets = if (all) holders.filterValues { mine in it }.keys.toList() else keys
            for (k in targets) holders[k]?.remove(mine)
            release(app, holders)
            onChange?.invoke()
        }
    }

    /**
     * An account's downloads all go from this device (decision 0101: "Remove this account's downloads" at its sign-out,
     * This device's Remove, an account Cratarr removed): its holds and its kept items; a file another account holds
     * stays.
     */
    fun releaseAccount(context: Context, who: String) {
        val app = context.applicationContext
        worker.execute {
            try {
                saveItems(app, loadItems(app).filter { it.who != who })
            } catch (e: IOException) {
                // As a keep: nothing goes on from a list that cannot be read, and nothing here ends the app.
                Log.w(TAG, "release: ${e.message}")
                return@execute
            }
            val holders = loadHolders(app)
            for (h in holders.values) h.removeAll { whoOf(it) == who }
            release(app, holders)
            onChange?.invoke()
        }
    }

    /** The downloads nothing holds any more removed, and the holders kept. */
    private fun release(app: Context, holders: MutableMap<String, MutableSet<String>>) {
        for (k in downloadedKeys(app)) if (holders[k].isNullOrEmpty()) {
            holders.remove(k)
            DownloadService.sendRemoveDownload(app, CratarrDownloadService::class.java, k, false)
        }
        try {
            saveHolders(app, holders)
        } catch (e: IOException) {
            Log.w(TAG, "release: ${e.message}")
        }
    }

    /**
     * The account signed in's downloads, by id: whether each is its own Download once (held by its `manual` alone), for
     * the Downloads page's Downloaded once; one its synced item holds too shows under Synced only (the owner,
     * 2026-10-24). One it does not hold is not in the map.
     */
    fun heldHere(context: Context): Map<String, Boolean> {
        val app = context.applicationContext
        val who = Who.current(app)
        val mine = held(who, MANUAL)
        return loadHolders(app).mapNotNull { (k, h) ->
            val its = h.filter { whoOf(it) == who }
            if (its.isEmpty()) null else k to (its == listOf(mine))
        }.toMap()
    }

    /** Each account's share of this device's downloads, in bytes (a file two hold counts in both), by its [Who] key. */
    private fun shares(holders: Map<String, Set<String>>, downloads: Map<String, Download>): Map<String, Long> {
        val out = mutableMapOf<String, Long>()
        for ((k, h) in holders) {
            val bytes = downloads[k]?.bytesDownloaded ?: continue
            for (who in h.map(::whoOf).toSet()) out[who] = (out[who] ?: 0L) + bytes
        }
        return out
    }

    /** WorkManager's every few hours, with the downloads' rules as its constraints. */
    fun schedule(context: Context) {
        val prefs = Prefs(context)
        val constraints = Constraints.Builder()
            .setRequiredNetworkType(if (prefs.downloadOverMobile) NetworkType.CONNECTED else NetworkType.UNMETERED)
            .setRequiresCharging(prefs.syncOnlyCharging)
            .build()
        val request = PeriodicWorkRequestBuilder<KeptWorker>(4, TimeUnit.HOURS).setConstraints(constraints).build()
        WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    // --- the sync ---

    private fun downloadedKeys(context: Context): List<String> {
        val out = mutableListOf<String>()
        Downloads.manager(context).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) out.add(c.download.request.id) }
        return out
    }

    private fun keyOf(t: JSONObject) = "${Uri.encode(t.optString("source"))}/${Uri.encode(t.optString("id"))}"

    private fun syncHere(app: Context, retryFailed: Boolean) {
        passes.incrementAndGet()
        onChange?.invoke()
        try {
            val prefs = Prefs(app)
            val server = prefs.serverUrl?.trimEnd('/')
            val who = Who.current(app)
            // A list that cannot be read ends the pass here, before anything is removed or saved.
            val items = loadItems(app)
            // Only the account signed in's items are asked after (its session asks); the others keep what they have.
            val ours = items.filter { it.who == who }
            if (server != null && ours.isNotEmpty()) ask(app, server, ours)

            // What is on the device now, and who holds it; a download without holders (just added, its hold on its
            // way) is the account signed in's.
            val downloads = mutableMapOf<String, Download>()
            Downloads.manager(app).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) downloads[c.download.request.id] = c.download }
            val holders = loadHolders(app)
            for (k in downloads.keys) if (holders[k].isNullOrEmpty()) holders[k] = mutableSetOf(held(who, MANUAL))
            val manual = holders.filterValues { h -> h.any(::isManual) }.keys

            // The budget: every account's downloads first, then the items, the newest kept first, each in its order.
            val budget = prefs.syncBudgetGb.let { if (it <= 0) Long.MAX_VALUE else it * 1_000_000_000L }
            val copies = prefs.downloadQuality == Prefs.QUALITY_SMALLER
            var used = manual.sumOf { downloads[it]?.bytesDownloaded ?: 0L }
            val wanted = linkedMapOf<String, MutableSet<String>>()
            val tracks = mutableMapOf<String, JSONObject>()
            for (item in items.sortedByDescending { it.since }) {
                var over = 0
                for (i in 0 until item.tracks.length()) {
                    val t = item.tracks.optJSONObject(i) ?: continue
                    val k = keyOf(t)
                    if (Previews.isPreview(k)) continue
                    // Another account's item holds only what is here already: nothing is fetched for it now.
                    if (item.who != who && k !in downloads) continue
                    tracks[k] = t
                    val holding = wanted[k]
                    if (holding != null) {
                        holding.add(item.holder)
                        continue
                    }
                    if (k in manual) {
                        wanted[k] = mutableSetOf(item.holder)
                        continue
                    }
                    val size = (if (copies) t.optLong("copy_size", -1) else t.optLong("size", -1)).takeIf { it > 0 }
                        ?: t.optDouble("duration_secs", Double.NaN).takeIf { !it.isNaN() }?.let { (it * 40_000).toLong() }
                        ?: UNKNOWN_SIZE
                    if (used + size <= budget) {
                        used += size
                        wanted[k] = mutableSetOf(item.holder)
                    } else {
                        over++
                    }
                }
                item.overBudget = over
            }

            // Holders as they are now: every account's own Downloads stay, the items' are what the budget gave.
            for (k in (holders.keys + wanted.keys).toSet()) {
                val h = (holders[k]?.filter(::isManual)?.toMutableSet() ?: mutableSetOf()).apply { addAll(wanted[k] ?: emptySet()) }
                if (h.isEmpty()) {
                    holders.remove(k)
                    if (k in downloads) DownloadService.sendRemoveDownload(app, CratarrDownloadService::class.java, k, false)
                } else {
                    holders[k] = h
                }
            }
            // New ones (and failed ones, when asked), as the owner's Download would add them.
            val missing = wanted.keys.filter { k ->
                val d = downloads[k]
                d == null || d.state == Download.STATE_REMOVING || (retryFailed && d.state == Download.STATE_FAILED)
            }
            if (server != null && missing.isNotEmpty()) {
                val added = System.currentTimeMillis() * 1000
                missing.forEachIndexed { i, k -> tracks[k]?.let { request(app, server, copies, it, added + i) } }
            }
            saveHolders(app, holders)
            saveItems(app, items)
        } catch (e: Exception) {
            Log.w(TAG, "sync: ${e.message}")
        } finally {
            passes.decrementAndGet()
        }
        onChange?.invoke()
    }

    /** The libraries the account signed in switched off, in its page settings here (`hiddenLibraries`, `source/library`
     *  keys; the person's own, decision 0101). */
    private fun librariesOff(app: Context): JSONArray =
        try {
            JSONObject(Prefs(app).pagePrefsOf(Who.current(app)) ?: "{}").optJSONArray("hiddenLibraries") ?: JSONArray()
        } catch (e: org.json.JSONException) {
            JSONArray()
        }

    /** Asks Cratarr what each item holds now; an unchanged one keeps its tracks. Offline: all stay as they were. */
    private fun ask(app: Context, server: String, items: List<Item>) {
        val body = JSONObject()
            .put("items", JSONArray(items.map { JSONObject().put("kind", it.kind).put("source", it.source).put("id", it.id) }))
            .put("versions", JSONObject().apply { for (i in items) i.version?.let { put(i.key, it) } })
            // The libraries switched off on this device, as its page keeps them: a crate's and an artist's tracks are those
            // of the libraries switched on, as they show (the owner, 2026-11-02).
            .put("without", librariesOff(app))
        val answer = try {
            val c = Net.open(app, "$server/api/kept")
            c.connectTimeout = 10_000
            c.readTimeout = 60_000
            c.requestMethod = "POST"
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toString().toByteArray()) }
            val code = Net.code(app, c)
            if (code != 200) throw IllegalStateException("Cratarr answered $code")
            JSONObject(c.inputStream.bufferedReader().readText())
        } catch (e: Exception) {
            Log.i(TAG, "asking Cratarr: ${e.message}")
            return
        }
        val byKey = items.associateBy { it.key }
        val list = answer.optJSONArray("items") ?: return
        for (n in 0 until list.length()) {
            val a = list.optJSONObject(n) ?: continue
            val item = byKey[a.optString("key")] ?: continue
            if (a.has("version")) {
                item.version = a.optString("version")
                item.error = null
                a.optJSONArray("tracks")?.let { item.tracks = it }
            } else {
                item.error = a.optString("error").ifEmpty { "Cratarr could not read it." }
            }
        }
    }

    private fun request(app: Context, server: String, copies: Boolean, t: JSONObject, added: Long) {
        val key = keyOf(t)
        val stored = if (copies) "$key#opus" else key
        val uri = Uri.parse("$server/api/file/$key" + if (copies) "?as=opus" else "")
        val data = JSONObject()
        for (f in listOf("source", "id", "name", "artist", "album", "album_id")) if (t.has(f) && !t.isNull(f)) data.put(f, t.get(f))
        data.put("added", added)
        val r = DownloadRequest.Builder(key, uri).setCustomCacheKey(stored).setData(data.toString().toByteArray()).build()
        DownloadService.sendAddDownload(app, CratarrDownloadService::class.java, r, false)
    }

    /**
     * Each kept track's album id, by its download's id (`source/id`), as Cratarr last sent the items' tracks. Downloads
     * requested before Cratarr sent album ids with a playlist's or a crate's tracks have none in their request; the
     * page's list takes it from here (Downloads.list), without asking for the download again.
     */
    fun albumIds(context: Context): Map<String, String> {
        val app = context.applicationContext
        return worker.submit<Map<String, String>> {
            val out = mutableMapOf<String, String>()
            for (item in shownItems(app)) for (i in 0 until item.tracks.length()) {
                val t = item.tracks.optJSONObject(i) ?: continue
                if (!t.isNull("album_id")) t.optString("album_id").takeIf { it.isNotEmpty() }?.let { out.putIfAbsent(keyOf(t), it) }
            }
            out
        }.get()
    }

    // --- the report for the page (the progress sheet) ---

    fun report(context: Context): JSONObject {
        val app = context.applicationContext
        val who = Who.current(app)
        val (items, holders) = worker.submit<Pair<List<Item>, Map<String, Set<String>>>> { shownItems(app).filter { it.who == who } to loadHolders(app) }.get()
        val downloads = mutableMapOf<String, Download>()
        Downloads.manager(app).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) downloads[c.download.request.id] = c.download }
        val manager = Downloads.manager(app)
        val notMet = manager.notMetRequirements
        val waiting = when {
            notMet and Requirements.DEVICE_CHARGING != 0 -> "charging"
            notMet and Requirements.NETWORK_UNMETERED != 0 -> "wifi"
            notMet and Requirements.NETWORK != 0 -> "network"
            else -> null
        }
        val current = manager.currentDownloads.firstOrNull { it.state == Download.STATE_DOWNLOADING }?.let {
            try {
                val t = JSONObject(String(it.request.data))
                listOf(t.optString("artist"), t.optString("name")).filter { s -> s.isNotEmpty() }.joinToString(" – ")
            } catch (e: Exception) {
                null
            }
        }
        // The sync job says its own state (0.13, the owner: no guessing from the items, nor a timed wait): running from a
        // pass's start until every kept track it asked for has come, also while its rules hold it back (`waiting`
        // says why). The owner's own Downloads are not the job's.
        val unfinished = setOf(Download.STATE_QUEUED, Download.STATE_DOWNLOADING, Download.STATE_RESTARTING)
        val running = passes.get() > 0 || items.any { item ->
            (0 until item.tracks.length()).any { i -> item.tracks.optJSONObject(i)?.let { downloads[keyOf(it)]?.state in unfinished } == true }
        }
        val out = JSONArray()
        for (item in items.sortedByDescending { it.since }) {
            var done = 0
            var failed = 0
            var bytes = 0L
            for (i in 0 until item.tracks.length()) {
                val d = item.tracks.optJSONObject(i)?.let { downloads[keyOf(it)] } ?: continue
                when (d.state) {
                    Download.STATE_COMPLETED -> done++
                    Download.STATE_FAILED -> failed++
                }
                bytes += d.bytesDownloaded
            }
            val total = item.tracks.length()
            val state = when {
                item.error != null && total == 0 -> "error"
                item.version == null -> "syncing"
                done + item.overBudget + failed >= total -> "done"
                waiting != null -> "waiting"
                else -> "syncing"
            }
            out.put(
                JSONObject()
                    .put("key", item.key).put("kind", item.kind).put("source", item.source).put("id", item.id).put("name", item.name)
                    .put("since", item.since).put("total", total).put("done", done).put("failed", failed)
                    .put("over_budget", item.overBudget).put("bytes", bytes).put("state", state).put("error", item.error ?: JSONObject.NULL),
            )
        }
        val budget = Prefs(app).syncBudgetGb
        // Each account's share (decision 0101), only while one is signed in: signed out, nothing of any account shows.
        val accounts = JSONArray()
        if (who != Who.NONE) for ((w, bytes) in shares(holders, downloads)) {
            if (w != Who.NONE) accounts.put(JSONObject().put("account", w.removePrefix("@")).put("bytes", bytes).put("here", w == who))
        }
        return JSONObject()
            .put("type", "kept")
            .put("items", out)
            .put("accounts", accounts)
            .put("used", downloads.values.sumOf { it.bytesDownloaded })
            .put("budget", if (budget <= 0) JSONObject.NULL else budget * 1_000_000_000L)
            .put("waiting", waiting ?: JSONObject.NULL)
            .put("running", running)
            .put("current", current ?: JSONObject.NULL)
    }
}

/** The kept lists as files: a stop or a full disk during a write must never read afterwards as nothing kept. */
internal object KeptFile {
    /** The list in [file]; none there yet is an empty one. One that is there and cannot be read throws. */
    fun list(file: File): List<JSONObject> {
        if (!file.exists()) return emptyList()
        return try {
            val a = JSONArray(file.readText())
            (0 until a.length()).map { a.getJSONObject(it) }
        } catch (e: JSONException) {
            throw IOException("${file.name} cannot be read: ${e.message}", e)
        }
    }

    /**
     * [text] in [file], whole or not at all: written aside by [put], then put in its place in one step. As LinkLog's trim,
     * but one that fails says so: `renameTo` answers false, and a save would be lost without a word.
     */
    fun write(file: File, text: String, put: (File, String) -> Unit = ::onDisk) {
        val aside = File(file.parentFile, "${file.name}.new")
        put(aside, text)
        Files.move(aside.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE)
    }

    /** On the disk itself before it takes the list's place, or a power cut could leave an empty file there. */
    private fun onDisk(file: File, text: String) = FileOutputStream(file).use { out ->
        out.write(text.toByteArray())
        out.fd.sync()
    }
}

/** Every few hours: the kept items synced (Kept.schedule). */
class KeptWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result = try {
        Kept.syncAndWait(applicationContext)
        Result.success()
    } catch (e: Exception) {
        Result.retry()
    }
}
