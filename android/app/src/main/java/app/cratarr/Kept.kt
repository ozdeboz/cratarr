/*
 * Kept on this device (decision 0038): albums, playlists, crates and artists the app keeps in step offline. A kept
 * item is followed: Cratarr says what it holds now (POST /api/kept, with the version last seen, so an unchanged item
 * costs a hash), its new tracks are downloaded and the ones that left it are removed, through the same download store
 * as the owner's own downloads (Downloads).
 * - A track goes only when nothing holds it: each download has its holders, each an account's ([Who], decision 0101):
 *   its `manual` (its Download) and its kept items' keys. A file two accounts hold is stored once; an account sees only
 *   what it holds, and its items are asked after only while it is signed in (the others keep what they have).
 * - One storage budget for the device (20 GB unless set; 0 is no limit). Every account's downloads count first and
 *   are never dropped, and what the items of the accounts away have here stays too (decision 0101, point 13: hidden but
 *   kept), whatever that leaves. Then the items of the one signed in, the newest kept first, each in its own order: what
 *   does not fit is left out, so the item kept longest ago shrinks first.
 * - Synced when the app starts, when an item is kept, when the settings change, when another's things are the device's
 *   ([Who.set]), and every few hours by WorkManager (KeptWorker), with the downloads' own rules: Wi-Fi unless mobile data is
 *   allowed, charging when asked. What is still downloading goes on only for one who holds it.
 * - The lists are written whole (KeptFile). A list, of the items or the holders, that is there and cannot be read stops
 *   the sync and any keep, and what it held stays on the device: read as nothing kept or held, the next sync or Remove
 *   would take it all, every account's. The page says so, and starts it again when asked ([startAgain]), the music staying.
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
    /** Why a download waits (Media3's stop reason, kept with it): no one holding it is signed in. */
    private const val HOLDER_AWAY = 1
    private const val WORK = "cratarr-kept"

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "cratarr-kept").apply { isDaemon = true } }

    /** Told after anything changed (MainActivity sends the page a report). */
    @Volatile var onChange: (() -> Unit)? = null

    /** Sync passes going on now (the report's `running`). */
    private val passes = java.util.concurrent.atomic.AtomicInteger(0)

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
        val holder get() = KeptHolds.held(who, key)

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

    /** Each download's holders ([KeptHolds]), by its id (`source/id`); throws when the list cannot be read, as [loadItems]. */
    private fun loadHolders(context: Context): MutableMap<String, MutableSet<String>> = KeptFile.holders(file(context, HOLDERS_FILE))

    /** The holders to show: none while the list cannot be read, so nothing shows as anyone's. */
    private fun shownHolders(context: Context): Map<String, Set<String>> = try {
        loadHolders(context)
    } catch (e: IOException) {
        Log.w(TAG, "the holders: ${e.message}")
        emptyMap()
    }

    /** Whether both lists read: one that does not stops everything here, and the page offers [startAgain]. */
    private fun readable(context: Context): Boolean = try {
        loadItems(context)
        loadHolders(context)
        true
    } catch (e: IOException) {
        false
    }

    private fun saveHolders(context: Context, holders: Map<String, Set<String>>) {
        val o = JSONObject()
        for ((k, v) in holders) if (v.isNotEmpty()) o.put(k, JSONArray(v.toList()))
        KeptFile.writeHolders(file(context, HOLDERS_FILE), o.toString())
    }

    // --- what the page and the other parts call ---

    /** Keep an item, or not, for the account signed in; then sync. */
    fun set(context: Context, kind: String, source: String, id: String, name: String, on: Boolean) {
        if (on && source == Previews.SOURCE) return
        val app = context.applicationContext
        // The account who asked, here and below: the worker may come to it behind a long pass, another signed in by then.
        val who = Who.current(app)
        worker.execute {
            try {
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

    /**
     * A list that cannot be read, started again (the owner, 2026-11-07), then a sync: one that reads is left as it is. Nothing on the
     * device goes: [KeptHolds.startAgain] says whose each download is from now on, by the last holders list that read where these do not.
     */
    fun startAgain(context: Context) {
        val app = context.applicationContext
        val who = Who.current(app)
        worker.execute {
            val items = try {
                loadItems(app)
            } catch (e: IOException) {
                null
            }
            val holders = try {
                loadHolders(app)
            } catch (e: IOException) {
                null
            }
            if (items != null && holders != null) return@execute
            val downloads = mutableListOf<String>()
            Downloads.manager(app).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) downloads.add(c.download.request.id) }
            val last = if (holders == null) KeptFile.lastHolders(file(app, HOLDERS_FILE)) else null
            try {
                saveHolders(app, KeptHolds.startAgain(holders, itemsGone = items == null, downloads, who, last))
                if (items == null) saveItems(app, emptyList())
            } catch (e: IOException) {
                // A full disk: the list stays as it was, and so does the offer.
                Log.w(TAG, "start again: ${e.message}")
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
        val mine = KeptHolds.held(Who.current(app), MANUAL)
        worker.execute {
            try {
                val holders = loadHolders(app)
                for (k in keys) holders.getOrPut(k) { mutableSetOf() }.add(mine)
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
        val mine = KeptHolds.held(Who.current(app), MANUAL)
        worker.execute {
            try {
                val holders = loadHolders(app)
                val targets = if (all) holders.filterValues { mine in it }.keys.toList() else keys
                release(app, holders, targets) { it == mine }
            } catch (e: IOException) {
                Log.w(TAG, "release: ${e.message}")
            }
            onChange?.invoke()
        }
    }

    /**
     * An account's downloads all go from this device (decision 0101: "Remove this account's downloads" at its sign-out,
     * This device's Remove): its holds and its kept items; a file another account holds stays. Point 13's account Cratarr
     * removed does not come here: the device cannot tell one from a session ended, each a 401.
     */
    fun releaseAccount(context: Context, who: String) {
        val app = context.applicationContext
        worker.execute {
            try {
                saveItems(app, loadItems(app).filter { it.who != who })
                val holders = loadHolders(app)
                val its = holders.filterValues { h -> h.any { KeptHolds.whoOf(it) == who } }.keys.toList()
                release(app, holders, its) { KeptHolds.whoOf(it) == who }
            } catch (e: IOException) {
                // As a keep: nothing goes on from a list that cannot be read, and nothing here ends the app.
                Log.w(TAG, "release: ${e.message}")
            }
            onChange?.invoke()
        }
    }

    /** The holds [gone] says on [keys] let go, the downloads nothing holds any more removed, and the holders kept. */
    private fun release(app: Context, holders: MutableMap<String, MutableSet<String>>, keys: Collection<String>, gone: (String) -> Boolean) {
        for (k in KeptHolds.letGo(holders, keys, gone)) DownloadService.sendRemoveDownload(app, CratarrDownloadService::class.java, k, false)
        saveHolders(app, holders)
    }

    /**
     * The account signed in's downloads, by id: whether each is its own Download once (held by its `manual` alone), for
     * the Downloads page's Downloaded once; one its synced item holds too shows under Synced only (the owner,
     * 2026-10-24). One it does not hold is not in the map.
     */
    fun heldHere(context: Context): Map<String, Boolean> {
        val app = context.applicationContext
        return KeptHolds.seenBy(shownHolders(app), Who.current(app))
    }

    /** What every account keeps and holds, as 0.16's fresh start writes it down ([FreshStart]): each kept item (its account, kind,
     *  source, id and name) and each download's holders. Null while either list cannot be read. */
    fun snapshot(context: Context): Pair<List<JSONObject>, Map<String, Set<String>>>? {
        val app = context.applicationContext
        return worker.submit<Pair<List<JSONObject>, Map<String, Set<String>>>?> {
            try {
                loadItems(app).map { JSONObject().put("who", it.who).put("kind", it.kind).put("source", it.source).put("id", it.id).put("name", it.name) } to loadHolders(app)
            } catch (e: IOException) {
                null
            }
        }.get()
    }

    /** Both lists emptied, for 0.16's fresh start ([FreshStart]); false where they could not be written. */
    fun forgetAll(context: Context): Boolean {
        val app = context.applicationContext
        return worker.submit<Boolean> {
            try {
                saveItems(app, emptyList())
                saveHolders(app, emptyMap())
                true
            } catch (e: IOException) {
                false
            }
        }.get()
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
            holdBack(app, who)
            // Only the account signed in's items are asked after (its session asks); the others keep what they have.
            val ours = items.filter { it.who == who }
            if (server != null && ours.isNotEmpty()) ask(app, server, ours)

            // What is on the device now, and who holds it. A download nothing holds is left alone: it is one just added, its
            // hold on its way, or one told to go. Given to the one signed in, that one would see and keep another's.
            val downloads = mutableMapOf<String, Download>()
            Downloads.manager(app).downloadIndex.getDownloads().use { c -> while (c.moveToNext()) downloads[c.download.request.id] = c.download }
            val holders = loadHolders(app)
            val manual = holders.filterValues { h -> h.any(KeptHolds::isManual) }.keys

            // The budget: every account's downloads first, then the items in their turn.
            val budget = prefs.syncBudgetGb.let { if (it <= 0) Long.MAX_VALUE else it * 1_000_000_000L }
            val copies = prefs.downloadQuality == Prefs.QUALITY_SMALLER
            val room = KeptRoom(budget, manual.sumOf { downloads[it]?.bytesDownloaded ?: 0L })
            val wanted = linkedMapOf<String, MutableSet<String>>()
            val tracks = mutableMapOf<String, JSONObject>()
            for (item in KeptHolds.inTurn(items, who, { it.who }, { it.since })) {
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
                    if (room.take(size, away = item.who != who)) {
                        wanted[k] = mutableSetOf(item.holder)
                    } else {
                        over++
                    }
                }
                item.overBudget = over
            }

            // Holders as they are now: every account's own Downloads stay, the items' are what the budget gave.
            for (k in KeptHolds.afterPass(holders, wanted)) {
                if (k in downloads) DownloadService.sendRemoveDownload(app, CratarrDownloadService::class.java, k, false)
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

    /** What is still coming goes on only for one who holds it ([KeptHolds.goesOn]), and takes up again at their return.
     *  Before anything is asked: each try of each file is a refusal until then. */
    private fun holdBack(app: Context, who: String) {
        val holders = loadHolders(app)
        Downloads.manager(app).downloadIndex.getDownloads(Download.STATE_QUEUED, Download.STATE_DOWNLOADING, Download.STATE_STOPPED).use { c ->
            while (c.moveToNext()) {
                val d = c.download
                val reason = if (KeptHolds.goesOn(holders[d.request.id], who)) Download.STOP_REASON_NONE else HOLDER_AWAY
                if (reason != d.stopReason) DownloadService.sendSetStopReason(app, CratarrDownloadService::class.java, d.request.id, reason, false)
            }
        }
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
        val (items, holders) = worker.submit<Pair<List<Item>, Map<String, Set<String>>>> { shownItems(app).filter { it.who == who } to shownHolders(app) }.get()
        val unreadable = !worker.submit<Boolean> { readable(app) }.get()
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
        val current = manager.currentDownloads.firstOrNull { it.state == Download.STATE_DOWNLOADING && KeptHolds.goesOn(holders[it.request.id], who) }?.let {
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
        if (who != Who.NONE) for ((w, bytes) in KeptHolds.shares(holders) { downloads[it]?.bytesDownloaded }) {
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
            .put("unreadable", unreadable)
    }
}

/** Who holds what (decision 0101), apart from the stores. A holder is [Who]'s key and what holds the download ([Kept.MANUAL]
 *  or an item's key), apart by a tab. */
/** One pass's room in the storage budget ([budget] bytes, [used] so far). */
internal class KeptRoom(private val budget: Long, private var used: Long) {
    /** A track of [size] bytes, if it has room. One of an account [away] always has: that account is not here to choose, and
     *  coming back it would find it gone, with no way to fetch it offline. Only the one signed in shrinks. */
    fun take(size: Long, away: Boolean): Boolean {
        if (!away && used + size > budget) return false
        used += size
        return true
    }
}

internal object KeptHolds {
    fun held(who: String, what: String) = "$who\t$what"

    fun whoOf(holder: String) = holder.substringBefore('\t')

    fun isManual(holder: String) = holder.substringAfter('\t') == Kept.MANUAL

    /** Whether a download not finished goes on with [who] signed in. It reads with whatever session there is: with none each
     *  file is refused until Media3 gives it up for good, and with another's it comes as that one's best copy, into a hold
     *  that chose otherwise. One nothing holds is left alone, as a pass leaves it: its hold is on its way, or it is going. */
    fun goesOn(holders: Set<String>?, who: String): Boolean = holders.isNullOrEmpty() || holders.any { whoOf(it) == who }

    /** The order a pass takes kept items in: the accounts' away first, what they have here staying; then those of [who], the
     *  one signed in, the newest kept first. */
    fun <T> inTurn(items: List<T>, who: String, whose: (T) -> String, since: (T) -> Long): List<T> =
        items.sortedWith(compareBy<T> { whose(it) == who }.thenByDescending(since))

    /** What [who] sees: by id, whether each is its own Download once (held by its `manual` alone). One it does not hold
     *  is not in the map. */
    fun seenBy(holders: Map<String, Set<String>>, who: String): Map<String, Boolean> {
        val mine = held(who, Kept.MANUAL)
        return holders.mapNotNull { (k, h) ->
            val its = h.filter { whoOf(it) == who }
            if (its.isEmpty()) null else k to (its == listOf(mine))
        }.toMap()
    }

    /** Each account's share of this device's downloads, in bytes (a file two hold counts in both), by its [Who] key. */
    fun shares(holders: Map<String, Set<String>>, bytesOf: (String) -> Long?): Map<String, Long> {
        val out = mutableMapOf<String, Long>()
        for ((k, h) in holders) {
            val bytes = bytesOf(k) ?: continue
            for (who in h.map(::whoOf).toSet()) out[who] = (out[who] ?: 0L) + bytes
        }
        return out
    }

    /** The holds [gone] says let go of on [keys]: those of them nothing holds now, out of [holders], for removing. Only
     *  these: a download no one was asked about may be one whose hold is on its way. */
    fun letGo(holders: MutableMap<String, MutableSet<String>>, keys: Collection<String>, gone: (String) -> Boolean): List<String> {
        val out = mutableListOf<String>()
        for (k in keys) {
            val h = holders[k] ?: continue
            h.removeAll(gone)
            if (h.isEmpty()) {
                holders.remove(k)
                out.add(k)
            }
        }
        return out
    }

    /**
     * Whose each download is once a list that could not be read starts again ([Kept.startAgain]), so that nothing on the device goes.
     * The holders gone: each download goes back to whoever held it in the [last] list that read (decision 0101: one account's things
     * never another's), and only one that list does not name is [who]'s own, as nothing then tells whose it was. The items gone: each
     * hold of an item becomes its account's own, so what the items kept stays, and shows, as downloaded once.
     */
    fun startAgain(
        holders: Map<String, Set<String>>?,
        itemsGone: Boolean,
        downloads: Collection<String>,
        who: String,
        last: Map<String, Set<String>>? = null,
    ): MutableMap<String, MutableSet<String>> {
        val own = { h: Set<String> -> if (itemsGone) h.mapTo(mutableSetOf()) { held(whoOf(it), Kept.MANUAL) } else h.toMutableSet() }
        if (holders == null) {
            return downloads.associateWith { d -> last?.get(d)?.takeIf { it.isNotEmpty() }?.let(own) ?: mutableSetOf(held(who, Kept.MANUAL)) }.toMutableMap()
        }
        return holders.mapValues { (_, h) -> own(h) }.toMutableMap()
    }

    /** After a pass: every account's own Downloads stay, the items' holds are what the budget gave ([wanted]). Those
     *  nothing holds now, out of [holders], for removing. */
    fun afterPass(holders: MutableMap<String, MutableSet<String>>, wanted: Map<String, Set<String>>): List<String> {
        val out = mutableListOf<String>()
        for (k in (holders.keys + wanted.keys).toSet()) {
            val h = (holders[k]?.filter(::isManual)?.toMutableSet() ?: mutableSetOf()).apply { addAll(wanted[k] ?: emptySet()) }
            if (h.isEmpty()) {
                holders.remove(k)
                out.add(k)
            } else {
                holders[k] = h
            }
        }
        return out
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

    /** The holders in [file], each download's, by its id; none there yet is an empty map. One that is there and cannot be
     *  read throws, as [list]: nothing held, every account's downloads would be free to go. A download with no holder
     *  of today's shape is left out, not read as held by nothing, which a pass would remove: as one nothing has heard of,
     *  it stays. */
    fun holders(file: File): MutableMap<String, MutableSet<String>> {
        if (!file.exists()) return mutableMapOf()
        return try {
            val o = JSONObject(file.readText())
            o.keys().asSequence().associateWith { k ->
                val a = o.getJSONArray(k)
                (0 until a.length()).map { a.getString(it) }.filter { '\t' in it }.toMutableSet()
            }.filterValues { it.isNotEmpty() }.toMutableMap()
        } catch (e: JSONException) {
            throw IOException("${file.name} cannot be read: ${e.message}", e)
        }
    }

    /**
     * The holders in [file] as [write] puts them, then mirrored as the last one ([lastHolders]): a list that cannot be read later still
     * tells from it whose each download was, the newest too (P7-REVIEW: one write behind, the newest would go to whoever signed in
     * next). A write is whole or not at all, so what a mirror copies is what was just written, never a list broken on the disk.
     */
    fun writeHolders(file: File, text: String, put: (File, String) -> Unit = ::onDisk) {
        write(file, text, put)
        write(lastOf(file), text, put)
    }

    /** The holders list last written whole; none where there is none, or it cannot be read either. */
    fun lastHolders(file: File): Map<String, Set<String>>? = lastOf(file).takeIf { it.exists() }?.let { runCatching { holders(it) }.getOrNull() }

    private fun lastOf(file: File) = File(file.parentFile, "${file.name.removeSuffix(".json")}.last.json")

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
