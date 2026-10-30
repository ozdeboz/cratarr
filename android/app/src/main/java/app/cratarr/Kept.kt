/*
 * Kept on this device (decision 0038): albums, playlists, crates and artists the app keeps in step offline. A kept
 * item is followed: Cratarr says what it holds now (POST /api/kept, with the version last seen, so an unchanged item
 * costs a hash), its new tracks are downloaded and the ones that left it are removed, through the same download store
 * as the owner's own downloads (Downloads).
 * - A track goes only when nothing holds it: each download has its holders, `manual` (the owner's Download) and the
 *   kept items' keys. A download from before this, without holders, counts as the owner's.
 * - One storage budget (20 GB unless set; 0 is no limit). The owner's downloads count first and are never dropped;
 *   then the items, the newest kept first, each in its own order: what does not fit is left out, so the item kept
 *   longest ago shrinks first.
 * - Synced when the app starts, when an item is kept, when the settings change, and every few hours by WorkManager
 *   (KeptWorker), with the downloads' own rules: Wi-Fi unless mobile data is allowed, charging when asked.
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
import org.json.JSONObject
import java.io.File
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

    /** A kept item, as stored: its tracks as Cratarr last said, and how many the budget left out. */
    private class Item(
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

        fun json(): JSONObject = JSONObject()
            .put("kind", kind).put("source", source).put("id", id).put("name", name).put("since", since)
            .put("version", version ?: JSONObject.NULL).put("tracks", tracks).put("error", error ?: JSONObject.NULL)
            .put("overBudget", overBudget)

        companion object {
            fun of(o: JSONObject) = Item(
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

    private fun loadItems(context: Context): MutableList<Item> = try {
        val a = JSONArray(file(context, ITEMS_FILE).readText())
        (0 until a.length()).map { Item.of(a.getJSONObject(it)) }.toMutableList()
    } catch (e: Exception) {
        mutableListOf()
    }

    private fun saveItems(context: Context, items: List<Item>) {
        file(context, ITEMS_FILE).writeText(JSONArray(items.map { it.json() }).toString())
    }

    /** Each download's holders, by its id (`source/id`). */
    private fun loadHolders(context: Context): MutableMap<String, MutableSet<String>> = try {
        val o = JSONObject(file(context, HOLDERS_FILE).readText())
        o.keys().asSequence().associateWith { k ->
            val a = o.getJSONArray(k)
            (0 until a.length()).map { a.getString(it) }.toMutableSet()
        }.toMutableMap()
    } catch (e: Exception) {
        mutableMapOf()
    }

    private fun saveHolders(context: Context, holders: Map<String, Set<String>>) {
        val o = JSONObject()
        for ((k, v) in holders) if (v.isNotEmpty()) o.put(k, JSONArray(v.toList()))
        file(context, HOLDERS_FILE).writeText(o.toString())
    }

    // --- what the page and the other parts call ---

    /** Keep an item, or not; then sync. */
    fun set(context: Context, kind: String, source: String, id: String, name: String, on: Boolean) {
        if (on && source == Previews.SOURCE) return
        val app = context.applicationContext
        worker.execute {
            val items = loadItems(app)
            val key = "$kind:$source/$id"
            items.removeAll { it.key == key }
            if (on) items.add(Item(kind, source, id, name.ifEmpty { id }, System.currentTimeMillis(), null, JSONArray(), null, 0))
            saveItems(app, items)
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

    /** The owner's Download: these tracks are held by `manual` from now on (Downloads.add calls it). */
    fun holdManual(context: Context, keys: List<String>) {
        val app = context.applicationContext
        worker.execute {
            val holders = loadHolders(app)
            for (k in keys) holders.getOrPut(k) { mutableSetOf() }.add(MANUAL)
            saveHolders(app, holders)
        }
    }

    /**
     * The owner removes downloads: their `manual` hold goes; a track a kept item still holds stays (it is shown as
     * kept), any other is removed.
     */
    fun releaseManual(context: Context, keys: List<String>, all: Boolean) {
        val app = context.applicationContext
        worker.execute {
            val holders = loadHolders(app)
            val targets = if (all) downloadedKeys(app) else keys
            for (k in targets) {
                val h = holders[k] ?: mutableSetOf()
                h.remove(MANUAL)
                if (h.isEmpty()) {
                    holders.remove(k)
                    DownloadService.sendRemoveDownload(app, CratarrDownloadService::class.java, k, false)
                } else {
                    holders[k] = h
                }
            }
            saveHolders(app, holders)
            onChange?.invoke()
        }
    }

    /**
     * The downloads that are the owner's own Download once (held by `manual` alone), for the Downloads page's Downloaded
     * once; one a synced item holds too shows under Synced only (the owner, 2026-10-24). One nothing holds is from
     * before holders were kept, and so the owner's.
     */
    fun manualKeys(context: Context): (String) -> Boolean {
        val holders = loadHolders(context.applicationContext)
        return { key -> holders[key]?.let { it.isEmpty() || it == setOf(MANUAL) } ?: true }
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
        try {
            val prefs = Prefs(app)
            val server = prefs.serverUrl?.trimEnd('/')
            val items = loadItems(app)
            if (server != null && items.isNotEmpty()) ask(app, server, items)

            // What is on the device now, and who holds it; a download without holders is the owner's.
            val downloads = mutableMapOf<String, Download>()
            Downloads.manager(app).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) downloads[c.download.request.id] = c.download }
            val holders = loadHolders(app)
            for (k in downloads.keys) if (holders[k].isNullOrEmpty()) holders[k] = mutableSetOf(MANUAL)
            val manual = holders.filterValues { MANUAL in it }.keys

            // The budget: the owner's downloads first, then the items, the newest kept first, each in its order.
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
                    tracks[k] = t
                    val held = wanted[k]
                    if (held != null) {
                        held.add(item.key)
                        continue
                    }
                    if (k in manual) {
                        wanted[k] = mutableSetOf(item.key)
                        continue
                    }
                    val size = (if (copies) t.optLong("copy_size", -1) else t.optLong("size", -1)).takeIf { it > 0 }
                        ?: t.optDouble("duration_secs", Double.NaN).takeIf { !it.isNaN() }?.let { (it * 40_000).toLong() }
                        ?: UNKNOWN_SIZE
                    if (used + size <= budget) {
                        used += size
                        wanted[k] = mutableSetOf(item.key)
                    } else {
                        over++
                    }
                }
                item.overBudget = over
            }

            // Holders as they are now: the owner's stay, the items' are what the budget gave.
            for (k in (holders.keys + wanted.keys).toSet()) {
                val h = (holders[k]?.filter { it == MANUAL }?.toMutableSet() ?: mutableSetOf()).apply { addAll(wanted[k] ?: emptySet()) }
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
        }
        onChange?.invoke()
    }

    /** Asks Cratarr what each item holds now; an unchanged one keeps its tracks. Offline: all stay as they were. */
    private fun ask(app: Context, server: String, items: List<Item>) {
        val body = JSONObject()
            .put("items", JSONArray(items.map { JSONObject().put("kind", it.kind).put("source", it.source).put("id", it.id) }))
            .put("versions", JSONObject().apply { for (i in items) i.version?.let { put(i.key, it) } })
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
            for (item in loadItems(app)) for (i in 0 until item.tracks.length()) {
                val t = item.tracks.optJSONObject(i) ?: continue
                if (!t.isNull("album_id")) t.optString("album_id").takeIf { it.isNotEmpty() }?.let { out.putIfAbsent(keyOf(t), it) }
            }
            out
        }.get()
    }

    // --- the report for the page (the progress sheet) ---

    fun report(context: Context): JSONObject {
        val app = context.applicationContext
        val items = worker.submit<List<Item>> { loadItems(app) }.get()
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
        return JSONObject()
            .put("type", "kept")
            .put("items", out)
            .put("used", downloads.values.sumOf { it.bytesDownloaded })
            .put("budget", if (budget <= 0) JSONObject.NULL else budget * 1_000_000_000L)
            .put("waiting", waiting ?: JSONObject.NULL)
            .put("current", current ?: JSONObject.NULL)
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
