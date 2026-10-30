/*
 * Cratarr's own (decision 0033, step 4): the tracks kept on the device, so music plays without a connection to Cratarr.
 * Two kinds share one store (Media3's cache, never evicted by itself):
 * - downloads the owner chose (an album, a playlist, a crate), through Media3's download manager and its service: as
 *   the files are, or as Cratarr's smaller copies (Opus), as the "This device" settings say;
 * - what comes next in the queue (`UP_NEXT` tracks ahead), kept as it is streamed anyway, and let go again once played.
 * The player reads every track through the store, so a kept one plays from it and any other from Cratarr.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.datasource.ResolvingDataSource
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.cache.CacheWriter
import androidx.media3.datasource.cache.ContentMetadata
import androidx.media3.datasource.cache.NoOpCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.offline.DownloadRequest
import androidx.media3.exoplayer.offline.DownloadService
import androidx.media3.exoplayer.scheduler.Requirements
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors

@OptIn(UnstableApi::class)
object Downloads {
    private const val TAG = "CratarrDownloads"
    /** Tracks ahead in the queue kept on the device. */
    private const val UP_NEXT = 5
    /** Tracks already played whose kept copy stays a while (a "previous" plays from the device too). */
    private const val KEEP_PLAYED = 2
    private const val AUTO_KEPT = "auto_kept"

    private var database: StandaloneDatabaseProvider? = null
    private var cache: SimpleCache? = null
    private var manager: DownloadManager? = null
    private val ahead = Executors.newSingleThreadExecutor()
    @Volatile private var writing: CacheWriter? = null

    @Synchronized
    private fun database(context: Context): StandaloneDatabaseProvider =
        database ?: StandaloneDatabaseProvider(context.applicationContext).also { database = it }

    /** The store: in the app's own folder on the device's storage, which goes with the app. */
    @Synchronized
    fun cache(context: Context): SimpleCache = cache ?: run {
        val app = context.applicationContext
        val dir = File(app.getExternalFilesDir(null) ?: app.filesDir, "tracks")
        SimpleCache(dir, NoOpCacheEvictor(), database(app)).also { cache = it }
    }

    /** Cratarr over HTTP, on the address in use: what was queued or asked for on the other one is read from this one.
     *  The player's covers come through it too. */
    fun http(context: Context): DataSource.Factory {
        val app = context.applicationContext
        val http = DefaultHttpDataSource.Factory()
            .setConnectTimeoutMs(15_000)
            .setReadTimeoutMs(60_000)
            .setAllowCrossProtocolRedirects(true)
        return ResolvingDataSource.Factory(http) { spec ->
            val uri = Reach.rewrite(app, spec.uri)
            val signed = spec.withUri(uri)
            // The session, and Play On's connection id (decision 0045), as the app's own requests carry them.
            val headers = listOfNotNull(Net.cookie(uri.toString())?.let { "Cookie" to it }, Net.connection?.let { "X-Cratarr-Connection" to it }).toMap()
            if (headers.isEmpty()) signed else signed.withAdditionalHeaders(headers)
        }
    }

    /** What the player reads tracks through: the store first, Cratarr for the rest. Streamed tracks are not stored. */
    fun playback(context: Context): CacheDataSource.Factory = CacheDataSource.Factory()
        .setCache(cache(context))
        .setUpstreamDataSourceFactory(http(context))
        .setCacheWriteDataSinkFactory(null)
        .setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)

    @Synchronized
    fun manager(context: Context): DownloadManager = manager ?: run {
        val app = context.applicationContext
        DownloadManager(app, database(app), cache(app), http(app), Executors.newFixedThreadPool(2)).also {
            it.maxParallelDownloads = 2
            it.requirements = requirements(Prefs(app))
            manager = it
        }
    }

    /** Over Wi-Fi only, unless the owner allows mobile data. */
    fun requirements(prefs: Prefs) = Requirements(
        (if (prefs.downloadOverMobile) Requirements.NETWORK else Requirements.NETWORK_UNMETERED) or
            (if (prefs.syncOnlyCharging) Requirements.DEVICE_CHARGING else 0),
    )

    /** A track's id in the store and in the download index: `source/id`, as its media item's. */
    private fun keyOf(track: JSONObject) = "${Uri.encode(track.optString("source"))}/${Uri.encode(track.optString("id"))}"

    /** The owner's downloads: tracks as the page describes them, in the quality the settings say. */
    fun add(context: Context, tracks: JSONArray) {
        val prefs = Prefs(context)
        val server = prefs.serverUrl?.trimEnd('/') ?: return
        val copies = prefs.downloadQuality == Prefs.QUALITY_SMALLER
        // The order they were given in, kept: an album's tracks list as the album runs.
        val added = System.currentTimeMillis() * 1000
        val held = mutableListOf<String>()
        for (i in 0 until tracks.length()) {
            val t = tracks.optJSONObject(i) ?: continue
            if (t.optString("source").isEmpty() || t.optString("id").isEmpty()) continue
            val key = keyOf(t)
            if (Previews.isPreview(key)) continue
            // Another quality is another file: kept under its own name, so the two never mix.
            val stored = if (copies) "$key#opus" else key
            val uri = Uri.parse("$server/api/file/$key" + if (copies) "?as=opus" else "")
            val request = DownloadRequest.Builder(key, uri)
                .setCustomCacheKey(stored)
                .setData(JSONObject(t.toString()).apply { remove("entry"); remove("asked_by"); put("added", added + i) }.toString().toByteArray())
                .build()
            DownloadService.sendAddDownload(context, CratarrDownloadService::class.java, request, false)
            held.add(key)
        }
        // The owner's from now on, whatever kept item also holds them (decision 0038).
        Kept.holdManual(context, held)
    }

    /** The owner's downloads go; a track a kept item holds stays (decision 0038). */
    fun remove(context: Context, ids: List<String>) = Kept.releaseManual(context, ids, all = false)

    fun removeAll(context: Context) = Kept.releaseManual(context, emptyList(), all = true)

    /** The owner's downloads, for the page: each track as it was given, with `download` (its id, state, size). */
    fun list(context: Context): JSONObject {
        val found = mutableListOf<JSONObject>()
        var bytes = 0L
        val albums = Kept.albumIds(context)
        val manual = Kept.manualKeys(context)
        manager(context).downloadIndex.getDownloads().use { c ->
            while (c.moveToNext()) {
                val d = c.download
                val t = try {
                    JSONObject(String(d.request.data))
                } catch (e: Exception) {
                    JSONObject()
                }
                // An older request without its album: the kept item's track has it (Kept.albumIds).
                if (t.isNull("album_id")) albums[d.request.id]?.let { t.put("album_id", it) }
                val state = when (d.state) {
                    Download.STATE_COMPLETED -> "done"
                    Download.STATE_FAILED -> "failed"
                    Download.STATE_DOWNLOADING -> "downloading"
                    else -> "waiting"
                }
                // Downloaded once by the owner, or there only for what is synced (decision 0038).
                t.put("manual", manual(d.request.id))
                t.put("download", JSONObject().put("id", d.request.id).put("state", state).put("bytes", d.bytesDownloaded).put("percent", d.percentDownloaded.toDouble()))
                found.add(t)
                bytes += d.bytesDownloaded
            }
        }
        val tracks = JSONArray(found.sortedBy { it.optLong("added") })
        return JSONObject().put("type", "downloads").put("tracks", tracks).put("bytes", bytes)
    }

    /** A track's item, its file from the store when it was downloaded whole, else from Cratarr (`server`): kept ahead
     *  whole in either form, that one; else on mobile data Cratarr's smaller copy (Opus; the owner, 2026-10-24), else
     *  the file as it is. A Bandcamp preview always from Cratarr, as it is (it has no smaller copy). */
    fun locate(context: Context, item: MediaItem, server: String?): MediaItem {
        val key = item.mediaId
        if (Previews.isPreview(key)) return if (server == null) item else item.buildUpon().setUri(fileUri(server, key, false)).build()
        val done = try {
            manager(context).downloadIndex.getDownload(key)?.takeIf { it.state == Download.STATE_COMPLETED }
        } catch (e: Exception) {
            null
        }
        val b = item.buildUpon()
        if (done != null) {
            b.setUri(done.request.uri).setCustomCacheKey(done.request.customCacheKey)
        } else if (server != null) {
            val store = cache(context.applicationContext)
            val opus = when {
                whole(store, key) -> false
                whole(store, "$key#opus") -> true
                else -> Net.metered(context)
            }
            b.setUri(fileUri(server, key, opus)).setCustomCacheKey(if (opus) "$key#opus" else key)
        }
        return b.build()
    }

    /** A track's file from Cratarr: as it is, or its smaller copy (kept under its own name, `#opus`). */
    private fun fileUri(server: String, key: String, opus: Boolean) = "$server/api/file/$key" + if (opus) "?as=opus" else ""

    /**
     * Keeps the next tracks of the queue on the device, one after the other, and lets go of those played some time
     * ago (unless the owner downloaded them). Called when the queue or the playing track changes.
     */
    fun keepAhead(context: Context, queue: List<MediaItem>, playing: Int) {
        val app = context.applicationContext
        val server = Prefs(app).serverUrl?.trimEnd('/') ?: return
        val wanted = queue.drop(playing.coerceAtLeast(0)).take(UP_NEXT + 1).map { it.mediaId }.filterNot(Previews::isPreview)
        // Near ones are kept in either form (the connection may have changed since).
        val keep = queue.drop((playing - KEEP_PLAYED).coerceAtLeast(0)).take(KEEP_PLAYED + UP_NEXT + 1)
            .flatMap { listOf(it.mediaId, "${it.mediaId}#opus") }.toSet()
        // On mobile data the smaller copies, as the playing track's (`locate`).
        val opus = Net.metered(app)
        writing?.cancel()
        ahead.execute {
            val store = cache(app)
            val kept = autoKept(app).toMutableList()
            // Let go of what is no longer near; a download of the owner's under the same name stays.
            for (stored in kept.filter { it !in keep }) {
                val downloaded = manager(app).downloadIndex.getDownload(stored.removeSuffix("#opus"))?.request?.customCacheKey == stored
                if (!downloaded) store.removeResource(stored)
                kept.remove(stored)
            }
            saveAutoKept(app, kept)
            for (key in wanted) {
                if (manager(app).downloadIndex.getDownload(key)?.state == Download.STATE_COMPLETED) continue
                if (whole(store, key) || whole(store, "$key#opus")) continue
                val stored = if (opus) "$key#opus" else key
                if (stored !in kept) {
                    kept.add(stored)
                    saveAutoKept(app, kept)
                }
                val spec = DataSpec.Builder().setUri(fileUri(server, key, opus)).setKey(stored).build()
                val source = CacheDataSource.Factory().setCache(store).setUpstreamDataSourceFactory(http(app)).createDataSourceForDownloading()
                val writer = CacheWriter(source, spec, null, null)
                writing = writer
                try {
                    writer.cache()
                } catch (e: Exception) {
                    // No connection, or the queue changed (cancelled): the next change tries again.
                    Log.i(TAG, "keeping $stored: ${e.message}")
                    return@execute
                } finally {
                    writing = null
                }
            }
        }
    }

    private fun whole(store: SimpleCache, key: String): Boolean {
        val length = ContentMetadata.getContentLength(store.getContentMetadata(key))
        return length > 0 && store.getCachedBytes(key, 0, length) >= length
    }

    private fun autoKept(context: Context): List<String> {
        val raw = Prefs.shared(context).getString(AUTO_KEPT, null) ?: return emptyList()
        val a = JSONArray(raw)
        return (0 until a.length()).map { a.getString(it) }
    }

    private fun saveAutoKept(context: Context, keys: List<String>) {
        Prefs.shared(context).edit().putString(AUTO_KEPT, JSONArray(keys).toString()).apply()
    }
}

/**
 * Bandcamp's previews (decision 0071): streamed from Cratarr (`/api/file/bandcamp/{id}`) like any track, never
 * downloaded, kept ahead or kept by an item. They are bought on Bandcamp.
 */
object Previews {
    const val SOURCE = "bandcamp"

    /** Whether a track's key (its media item's id, `source/id`) is a preview's. */
    fun isPreview(key: String) = key.startsWith("$SOURCE/")
}
