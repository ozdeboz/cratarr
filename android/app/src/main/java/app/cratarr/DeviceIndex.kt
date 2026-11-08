/*
 * The library on this device, kept up to date by the app (PREVIEW2's D15; the owner's Q3): so a car or a sleeping WebView still has
 * it, where a page in a browser keeps its own (web/src/lib/deviceSync). Whole once, then what changes. What to do, and what each
 * answer and each set of bytes came to, are the rule's (`index::sync` through UniFFI, the one the page asks), so this only fetches,
 * keeps and tells.
 * - What is kept is bytes (the owner's Q1), each account's apart ([Who]), hidden while another is signed in and there at its next
 *   sign-in: for each part a file of bytes and a small one of how it stands. Bytes first: a stop between the two leaves bytes ahead
 *   of their word, which a feed refuses, and a whole mends. Small covers are a file each, and come a frame at a time: a bundle can be
 *   tens of MB.
 * - Looked at a few seconds into a start (the app or its service opening, another account, hours away), on a new network, at the
 *   page's word, and every few hours by WorkManager, on any connection: the rule says what each may fetch.
 * - A device holds three revs on one log, and no feed is given another's. Rows' and sounds' feeds are each a whole build on the server,
 *   so a look asks them only when the log says something moved (`/api/changes`, the page's word or its own asking). Covers' builds
 *   nothing, and asking it is what sets the maker going, so it is asked at every look. An account's own has no feed: the rule says
 *   when it has stood long enough to fetch again.
 * - Waits are kept only with the page in view. Out of view a look that must wait ends there, as asking every half minute from a car
 *   with Cratarr away would only cost the battery: the next network, start or worker looks again.
 * - The page reads the bytes through [PAGE_PATH] with its own index.wasm: one reader in a browser and here, and nothing here holds
 *   the library in memory between looks. Its row is told facts ([report]), and says the word itself.
 * All of it runs on one thread.
 */
package app.cratarr

import android.content.Context
import android.util.Log
import android.webkit.WebResourceResponse
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import app.cratarr.rules.SyncKind
import app.cratarr.rules.SyncMobile
import app.cratarr.rules.SyncNetwork
import app.cratarr.rules.SyncOutcome
import app.cratarr.rules.SyncPart
import app.cratarr.rules.SyncStep
import app.cratarr.rules.SyncTaken
import app.cratarr.rules.syncAfter
import app.cratarr.rules.syncAnew
import app.cratarr.rules.syncAnewAfterMs
import app.cratarr.rules.syncAnewByAnother
import app.cratarr.rules.syncAnswer
import app.cratarr.rules.syncFirstAfterMs
import app.cratarr.rules.syncPaused
import app.cratarr.rules.syncStep
import app.cratarr.rules.syncStepStale
import app.cratarr.rules.syncTakeChanges
import app.cratarr.rules.syncTakeWhole
import app.cratarr.rules.syncVersion
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.net.URLEncoder
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

object DeviceIndex {
    private const val TAG = "CratarrIndex"
    private const val DIR = "device-index"
    private const val WORK = "cratarr-index"
    private const val EVERY_HOURS = 4L
    private const val INDEX = "index"

    /** Where the page reads what is kept of the index (web/src/lib/deviceIndex). */
    const val PAGE_PATH = "/__device/index"

    /** Where the page's service worker asks for a record's small cover, by its number (public/sw.js): a browser's opens them itself. */
    const val COVER_PATH = "/__device/index/cover/"

    private val worker = Executors.newSingleThreadScheduledExecutor { Thread(it, "cratarr-index").apply { isDaemon = true } }

    /** Told after anything changed (MainActivity sends the page a report). */
    @Volatile var onChange: (() -> Unit)? = null

    private var looks: IndexLooks? = null
    private var due: ScheduledFuture<*>? = null
    /** The parts as the last look left them; none before one. */
    @Volatile private var facts: List<IndexFacts> = emptyList()
    @Volatile private var inView = false
    @Volatile private var hiddenAt: Long? = null

    private fun files(app: Context) = IndexFiles(File(app.filesDir, DIR))

    private fun looks(app: Context) = looks ?: IndexLooks(
        RustIndexRule,
        files(app),
        { path, read -> opened(app, path, read) },
        { Who.current(app) },
        { networkNow(app) },
        { mobileNow(app) },
        System::currentTimeMillis,
    ).also { looks = it }

    /** A device with no rules library (32-bit ARM) keeps no index, and a failure of any look is no reason to end the app. */
    private inline fun guarded(work: () -> Unit) {
        try {
            work()
        } catch (e: Throwable) {
            Log.w(TAG, "a look: $e")
        }
    }

    private fun networkNow(app: Context): SyncNetwork = when {
        !NetState.connected(app) -> SyncNetwork.OFF
        Net.metered(app) -> SyncNetwork.METERED
        else -> SyncNetwork.FREE
    }

    /** What the person said of a whole fetch on mobile data: the page's setting, which the app keeps a copy of. */
    private fun mobileNow(app: Context): SyncMobile = try {
        when (JSONObject(Prefs(app).pagePrefs ?: "{}").optString("indexMobile")) {
            "yes" -> SyncMobile.YES
            "no" -> SyncMobile.NO
            else -> SyncMobile.UNASKED
        }
    } catch (e: JSONException) {
        SyncMobile.UNASKED
    }

    /** Asks Cratarr for [path]: [read] has its status (null: no answer at all), and its body where it has one, open only meanwhile. */
    private fun opened(app: Context, path: String, read: (Int?, InputStream?) -> Unit) {
        val server = Prefs(app).serverUrl?.trimEnd('/') ?: return read(null, null)
        try {
            val c = Net.open(app, server + path)
            try {
                c.connectTimeout = 10_000
                c.readTimeout = 60_000
                val code = Net.code(app, c)
                read(code, if (code in 200..299 && code != 204) c.inputStream else null)
            } finally {
                c.disconnect()
            }
        } catch (e: IOException) {
            read(null, null)
        }
    }

    /** WorkManager's every few hours. Not the kept items' worker, whose rules are the downloads': changes go on any connection. */
    fun schedule(context: Context) {
        val constraints = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
        val request = PeriodicWorkRequestBuilder<IndexWorker>(EVERY_HOURS, TimeUnit.HOURS).setConstraints(constraints).build()
        WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    private fun lookAt(app: Context, at: Long, told: Boolean = false) {
        due?.cancel(false)
        due = worker.schedule({ lookHere(app, told) }, maxOf(0L, at - System.currentTimeMillis()), TimeUnit.MILLISECONDS)
    }

    private fun startHere(app: Context) {
        looks(app).startAnew()
        facts = emptyList()
        onChange?.invoke()
        lookAt(app, System.currentTimeMillis() + syncFirstAfterMs())
    }

    private fun lookHere(app: Context, told: Boolean = false) {
        guarded {
            val server = Prefs(app).serverUrl
            val who = Who.current(app)
            if (server != null && who != Who.NONE && !Net.refused(app, server)) {
                val looks = looks(app)
                val steps = looks.look(who, told)
                facts = looks.facts(steps)
                if (inView) steps.values.filterIsInstance<SyncStep.Wait>().minOfOrNull { it.until }?.let { lookAt(app, it, told) }
                onChange?.invoke()
            }
        }
    }

    /** A start: another account's things are the device's, or the app or its service opened. */
    fun start(context: Context) {
        val app = context.applicationContext
        worker.execute { guarded { startHere(app) } }
    }

    /** The library kept on this device let go, every account's (0.16's one fresh start, [FreshStart]), and its looks begun again. */
    fun forget(context: Context) {
        val app = context.applicationContext
        worker.submit {
            guarded {
                File(app.filesDir, DIR).deleteRecursively()
                startHere(app)
            }
        }.get()
    }

    /** Looks now: the page's word, or a new network. [told]: the log said rows or sounds moved, and need not be asked. */
    fun look(context: Context, told: Boolean = false) {
        val app = context.applicationContext
        worker.execute { lookHere(app, told) }
    }

    /** The row tapped: what stopped tries again. */
    fun tryAgain(context: Context) {
        val app = context.applicationContext
        worker.execute {
            guarded { looks(app).tryAgain() }
            lookHere(app)
        }
    }

    /** The page in view: hours away count as a start. */
    fun shown(context: Context) {
        val app = context.applicationContext
        val away = hiddenAt
        inView = true
        hiddenAt = null
        worker.execute {
            guarded {
                if (away != null && System.currentTimeMillis() - away >= syncAnewAfterMs()) startHere(app)
            }
        }
    }

    fun hidden() {
        inView = false
        hiddenAt = System.currentTimeMillis()
    }

    /** A worker's look, and wait: a start of its own, its first seconds waited out. */
    fun syncAndWait(context: Context) {
        val app = context.applicationContext
        val first = worker.submit<Long> {
            looks(app).startAnew()
            syncFirstAfterMs()
        }.get()
        Thread.sleep(first)
        worker.submit { lookHere(app) }.get(10, TimeUnit.MINUTES)
    }

    /** How it stands, for the page: facts, as the word is the page's (lib/deviceIndexRule's `indexShows`). */
    fun report(context: Context): JSONObject {
        val app = context.applicationContext
        val who = Who.current(app)
        val files = files(app)
        val index = files.stands(who, INDEX)
        val known = facts.ifEmpty { INDEX_PARTS.filterNot { it.stale }.mapNotNull { d -> files.stands(who, d.name)?.let { IndexFacts(it.part.whole, it.part.stopped, paused = false, asks = false) } } }
        return JSONObject()
            .put("type", "device-index")
            .put("at", index?.at ?: JSONObject.NULL)
            .put("rev", index?.rev ?: JSONObject.NULL)
            .put("covers", files.stands(who, "covers")?.rev ?: JSONObject.NULL)
            .put("metered", Net.metered(app))
            .put("parts", JSONArray(known.map { JSONObject().put("whole", it.whole).put("stopped", it.stopped).put("paused", it.paused).put("asks", it.asks) }))
    }

    /** A part's bytes as kept for the account signed in, for what answers for itself. */
    fun kept(context: Context, name: String): ByteArray? {
        val app = context.applicationContext
        return files(app).bytes(Who.current(app), name).takeIf { it.exists() }?.readBytes()
    }

    /** Where what it reads stands: moved, it is read anew. */
    fun stands(context: Context): List<Long?> {
        val app = context.applicationContext
        val (who, files) = Who.current(app) to files(app)
        return INDEX_PARTS.filterNot { it.frames }.flatMap { d -> files.stands(who, d.name).let { listOf(it?.rev, it?.at) } }
    }

    /** What is kept for the account signed in, by its address: the index ([PAGE_PATH]) or a small cover ([COVER_PATH]). Null for any other. */
    fun answer(context: Context, path: String): WebResourceResponse? {
        val app = context.applicationContext
        return if (path == PAGE_PATH) index(app) else coverRecord(path)?.let { cover(app, it) }
    }

    /** The index, for the page: its bytes, and when it last stood where Cratarr does. */
    private fun index(app: Context): WebResourceResponse {
        val who = Who.current(app)
        val files = files(app)
        val bytes = files.bytes(who, INDEX).takeIf { it.exists() }
        val stands = files.stands(who, INDEX)
        if (bytes == null || stands == null) return nothing()
        val headers = mapOf("Cache-Control" to "no-store", "X-Cratarr-At" to (stands.at ?: 0L).toString())
        return WebResourceResponse("application/octet-stream", null, 200, "OK", headers, FileInputStream(bytes))
    }

    private fun cover(app: Context, record: Long): WebResourceResponse {
        val file = files(app).cover(Who.current(app), record).takeIf { it.exists() } ?: return nothing()
        return WebResourceResponse("image/jpeg", null, 200, "OK", mapOf("Cache-Control" to "no-store"), FileInputStream(file))
    }

    private fun nothing() = WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
}

/** The record a small cover's address names; null where it names none. */
internal fun coverRecord(path: String): Long? = path.takeIf { it.startsWith(DeviceIndex.COVER_PATH) }?.removePrefix(DeviceIndex.COVER_PATH)?.toLongOrNull()

/** Every few hours: the library kept up to date, with no page open ([DeviceIndex.schedule]). */
class IndexWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result = try {
        DeviceIndex.syncAndWait(applicationContext)
        Result.success()
    } catch (e: Exception) {
        Result.retry()
    }
}

/** What a part is, to fetching it, as in lib/deviceSync. [frames]: it comes a frame at a time, is kept by the frame, and its feed builds
 *  nothing, so it is asked at every look. [stale]: it has no feed, and is fetched whole when it has stood a while. */
internal class IndexDef(val name: String, val kind: SyncKind, val whole: String, val feed: String, val frames: Boolean = false, val stale: Boolean = false)

internal val INDEX_PARTS = listOf(
    IndexDef("index", SyncKind.INDEX, "/api/index", "/api/index/changes"),
    IndexDef("sounds", SyncKind.SOUNDS, "/api/index/sounds", "/api/index/sounds/changes"),
    IndexDef("covers", SyncKind.COVERS, "/api/index/covers", "/api/index/covers/changes", frames = true),
    IndexDef("own", SyncKind.OWN, "/api/index/own", "/api/index/own", stale = true),
)

/** What a fetch brought: its status (null: no answer at all), and its bytes where it had any. */
internal class IndexFetched(val status: Int?, val bytes: ByteArray?)

/** The rule, as the rules crate says it. In the unit tests, which have no library, a stand-in: all it proves there is what is done with its answers. */
internal interface IndexRule {
    fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, told: Boolean, now: Long): SyncStep
    fun stepStale(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, took: Long?, now: Long): SyncStep
    fun after(part: SyncPart, outcome: SyncOutcome, now: Long): SyncPart
    fun anew(part: SyncPart, now: Long): SyncPart
    fun anewByAnother(part: SyncPart, now: Long): SyncPart
    fun paused(part: SyncPart): Boolean
    fun answer(status: Int?, whole: Boolean): SyncOutcome?
    fun version(kind: SyncKind): Int
    fun takeWhole(kind: SyncKind, bytes: ByteArray): SyncTaken
    fun takeChanges(kind: SyncKind, kept: ByteArray, feed: ByteArray): SyncTaken
}

internal object RustIndexRule : IndexRule {
    override fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, told: Boolean, now: Long) = syncStep(part, network, mobile, told, now)
    override fun stepStale(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, took: Long?, now: Long) = syncStepStale(part, network, mobile, took, now)
    override fun after(part: SyncPart, outcome: SyncOutcome, now: Long) = syncAfter(part, outcome, now)
    override fun anew(part: SyncPart, now: Long) = syncAnew(part, now)
    override fun anewByAnother(part: SyncPart, now: Long) = syncAnewByAnother(part, now)
    override fun paused(part: SyncPart) = syncPaused(part)
    override fun answer(status: Int?, whole: Boolean) = syncAnswer(status?.toUShort(), whole)
    override fun version(kind: SyncKind) = syncVersion(kind).toInt()
    override fun takeWhole(kind: SyncKind, bytes: ByteArray) = syncTakeWhole(kind, bytes)
    override fun takeChanges(kind: SyncKind, kept: ByteArray, feed: ByteArray) = syncTakeChanges(kind, kept, feed)
}

/** How a part stands as kept: where it stands, when it last stood where Cratarr does (ms), the version of the app that wrote it, and
 *  how its fetching stands (`started` is each start's own, and not kept). */
internal class IndexStands(val rev: Long?, val at: Long?, val wants: Int?, val part: SyncPart)

/** A part as the page is told it. */
internal class IndexFacts(val whole: Boolean, val stopped: Boolean, val paused: Boolean, val asks: Boolean)

/** The kept files, under [root]: each account's in a folder of its own. */
internal class IndexFiles(private val root: File) {
    private fun folder(who: String) = File(root, URLEncoder.encode(who, "UTF-8"))

    fun bytes(who: String, name: String) = File(folder(who), "$name.bin")

    private fun covers(who: String) = File(folder(who), "covers")

    /** A record's small cover. */
    fun cover(who: String, record: Long) = File(covers(who), "$record.jpg")

    /** Takes a record's small cover, or lets it go where it has none now. Not to the disk itself as [onDisk] is: a next whole mends one
     *  a power cut took, and a library is thousands. */
    fun putCover(who: String, record: Long, jpeg: ByteArray?) {
        val file = cover(who, record)
        if (jpeg == null) {
            file.delete()
            return
        }
        covers(who).mkdirs()
        replace(file, jpeg) { to, bytes -> to.writeBytes(bytes) }
    }

    /** Small covers of [who] kept only for [records]: once a whole is all in, what it no longer holds goes. */
    fun keepCoversOf(who: String, records: Set<Long>) {
        covers(who).listFiles()?.forEach { f -> if (f.name.removeSuffix(".jpg").toLongOrNull() !in records) f.delete() }
    }

    private fun standing(who: String, name: String) = File(folder(who), "$name.json")

    /** How a part stands; null where nothing is, or it cannot be read: this is a copy of what Cratarr holds, and a whole mends it. */
    fun stands(who: String, name: String): IndexStands? {
        val file = standing(who, name)
        if (!file.exists()) return null
        return try {
            val o = JSONObject(file.readText())
            val part = SyncPart(o.getBoolean("whole"), o.getBoolean("stopped"), o.getInt("lost").toUInt(), o.getInt("failed").toUInt(), o.long("tried"), 0L)
            IndexStands(o.long("rev"), o.long("at"), o.long("wants")?.toInt(), part)
        } catch (e: JSONException) {
            null
        } catch (e: IOException) {
            null
        }
    }

    private fun JSONObject.long(name: String): Long? = if (isNull(name)) null else getLong(name)

    /** Keeps how a part stands, and new [bytes] if it has any: bytes first (the file's head says why). Throws where they cannot be
     *  written; what was there stays whole. */
    fun keep(who: String, name: String, stands: IndexStands, bytes: ByteArray?, put: (File, ByteArray) -> Unit = ::onDisk) {
        folder(who).mkdirs()
        bytes?.let { replace(bytes(who, name), it, put) }
        val p = stands.part
        val text = JSONObject()
            .put("rev", stands.rev ?: JSONObject.NULL).put("at", stands.at ?: JSONObject.NULL).put("wants", stands.wants ?: JSONObject.NULL)
            .put("whole", p.whole).put("stopped", p.stopped).put("lost", p.lost.toInt()).put("failed", p.failed.toInt()).put("tried", p.tried ?: JSONObject.NULL)
        replace(standing(who, name), text.toString().toByteArray(), put)
    }

    /** [file] holds [bytes], whole or not at all: written aside, then put in its place in one step. */
    private fun replace(file: File, bytes: ByteArray, put: (File, ByteArray) -> Unit) {
        val aside = File(file.parentFile, "${file.name}.new")
        put(aside, bytes)
        Files.move(aside.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE)
    }

    private fun onDisk(file: File, bytes: ByteArray) = FileOutputStream(file).use { out ->
        out.write(bytes)
        out.fd.sync()
    }
}

/** The looks themselves, with nothing of Android's in them: as lib/deviceSync's, which keeps its bytes in a browser. */
internal class IndexLooks(
    private val rule: IndexRule,
    private val files: IndexFiles,
    private val open: (String, (Int?, InputStream?) -> Unit) -> Unit,
    private val whoNow: () -> String,
    private val network: () -> SyncNetwork,
    private val mobile: () -> SyncMobile,
    private val now: () -> Long,
) {
    /** When this start began, and how each part stands in it. */
    private var started = now()
    private val parts = mutableMapOf<String, SyncPart>()

    /** What a fetch came to, and what of a part is kept for it. */
    private class Got(val outcome: SyncOutcome, val bytes: ByteArray? = null, val rev: Long? = null, val at: Long? = null)

    /** A start: what went wrong is forgotten. */
    fun startAnew() {
        started = now()
        parts.clear()
    }

    /** The person's tap: what stopped tries again. */
    fun tryAgain() {
        for ((name, part) in parts.toMap()) if (part.stopped) parts[name] = part.copy(stopped = false)
    }

    /** Each part as far as it goes now, for [who]: what it was last told to do. [told]: the log has said rows or sounds moved. */
    fun look(who: String, told: Boolean = false): Map<String, SyncStep> {
        val moved = told || moved(who)
        return INDEX_PARTS.associate { it.name to bringUp(it, who, moved || it.frames) }
    }

    /** What the log says of the rows held: one small answer, where their feeds are each a whole build. Nothing held: nothing to ask. */
    private fun moved(who: String): Boolean {
        val rev = files.stands(who, INDEX_PARTS[0].name)?.rev ?: return false
        val bytes = fetched("/api/changes?since=0&kinds=index&index=$rev").bytes ?: return false
        return try {
            JSONObject(String(bytes)).optJSONObject("index")?.optBoolean("changed") == true
        } catch (e: JSONException) {
            false
        }
    }

    private fun fetched(path: String): IndexFetched {
        var got = IndexFetched(null, null)
        open(path) { status, body ->
            got = try {
                IndexFetched(status, body?.readBytes())
            } catch (e: IOException) {
                IndexFetched(null, null)
            }
        }
        return got
    }

    /** The parts as this look left them. */
    fun facts(steps: Map<String, SyncStep>): List<IndexFacts> = INDEX_PARTS.filterNot { it.stale }.mapNotNull { d ->
        parts[d.name]?.let { IndexFacts(it.whole, it.stopped, rule.paused(it), steps[d.name] == SyncStep.Ask) }
    }

    /** A part as this start takes it from what is kept. */
    private fun partOf(def: IndexDef, kept: IndexStands): SyncPart = parts.getOrPut(def.name) {
        val stood = kept.part.copy(started = started)
        if (kept.wants == rule.version(def.kind)) rule.anew(stood, started) else rule.anewByAnother(stood, started)
    }

    private fun whole(def: IndexDef): Got {
        val got = fetched(def.whole)
        rule.answer(got.status, true)?.let { return Got(it) }
        val bytes = got.bytes ?: return Got(SyncOutcome.LOST)
        val taken = rule.takeWhole(def.kind, bytes)
        return if (taken.outcome == SyncOutcome.TOOK) Got(SyncOutcome.TOOK, bytes, taken.rev, now()) else Got(taken.outcome)
    }

    private fun changes(def: IndexDef, who: String, kept: IndexStands): Got {
        val file = files.bytes(who, def.name)
        if (kept.rev == null || !file.exists()) return Got(SyncOutcome.START_AGAIN)
        val got = fetched("${def.feed}?since=${kept.rev}")
        rule.answer(got.status, false)?.let { return if (it == SyncOutcome.TOOK) Got(it, at = now()) else Got(it) }
        val feed = got.bytes ?: return Got(SyncOutcome.LOST)
        val taken = rule.takeChanges(def.kind, file.readBytes(), feed)
        return if (taken.outcome == SyncOutcome.TOOK) Got(SyncOutcome.TOOK, taken.bytes, taken.rev, now()) else Got(taken.outcome)
    }

    /** Covers' head to the rule. Empty, it says they were cut short. */
    private fun taken(def: IndexDef, head: ByteArray, whole: Boolean) = if (whole) rule.takeWhole(def.kind, head) else rule.takeChanges(def.kind, ByteArray(0), head)

    /** As much of [into] as [body] has; how much that was. */
    private fun fill(body: InputStream, into: ByteArray): Int {
        var n = 0
        while (n < into.size) {
            val read = body.read(into, n, into.size - n)
            if (read < 0) break
            n += read
        }
        return n
    }

    /** Covers as they come, each to its file: the form is plain, little-endian, read to the end of the body (the server's device_covers). */
    private fun framesOf(def: IndexDef, who: String, whole: Boolean, body: InputStream): Got {
        val head = ByteArray(COVERS_HEAD)
        val taken = taken(def, head.copyOf(fill(body, head)), whole)
        if (taken.outcome != SyncOutcome.TOOK) return Got(taken.outcome)
        // A whole's records: what it no longer holds goes once it is all in, never before, so one cut short leaves what was held
        // (audit/OFFLINE-REVIEW.md, finding 4).
        val records = if (whole) mutableSetOf<Long>() else null
        val frame = ByteArray(COVER_FRAME)
        while (true) {
            val read = fill(body, frame)
            if (read == 0) {
                records?.let { files.keepCoversOf(who, it) }
                return Got(SyncOutcome.TOOK, rev = taken.rev, at = now())
            }
            if (read < COVER_FRAME) break
            val fields = ByteBuffer.wrap(frame).order(ByteOrder.LITTLE_ENDIAN)
            val record = fields.getLong()
            val length = fields.getInt()
            if (length < 0) break
            val jpeg = ByteArray(length)
            if (fill(body, jpeg) < length) break
            files.putCover(who, record, jpeg.takeIf { length > 0 })
            records?.add(record)
        }
        return Got(taken(def, ByteArray(0), whole).outcome)
    }

    private fun framed(def: IndexDef, who: String, whole: Boolean, kept: IndexStands): Got {
        if (!whole && kept.rev == null) return Got(SyncOutcome.START_AGAIN)
        var got = Got(SyncOutcome.LOST)
        open(if (whole) def.whole else "${def.feed}?since=${kept.rev}") { status, body ->
            val ended = rule.answer(status, whole)
            got = when {
                ended == SyncOutcome.TOOK -> Got(ended, at = now())
                ended != null -> Got(ended)
                body == null -> Got(SyncOutcome.LOST)
                else -> try {
                    framesOf(def, who, whole, body)
                } catch (e: IOException) {
                    Got(SyncOutcome.LOST)
                }
            }
        }
        return got
    }

    /** One part as far as it goes now: what it was last told to do. */
    private fun bringUp(def: IndexDef, who: String, told: Boolean): SyncStep {
        var turn = 0
        while (true) {
            if (whoNow() != who) return SyncStep.Rest
            val kept = files.stands(who, def.name) ?: NOTHING
            val part = partOf(def, kept)
            val step = if (def.stale) rule.stepStale(part, network(), mobile(), kept.at, now()) else rule.step(part, network(), mobile(), told, now())
            if ((step != SyncStep.Whole && step != SyncStep.Changes) || turn == TURNS) return step
            val got = when {
                def.frames -> framed(def, who, step == SyncStep.Whole, kept)
                step == SyncStep.Whole -> whole(def)
                else -> changes(def, who, kept)
            }
            // Another account came meanwhile: what was fetched is the one before's.
            if (whoNow() != who) return SyncStep.Rest
            val after = rule.after(part, got.outcome, now())
            parts[def.name] = after
            val stands = IndexStands(got.rev ?: kept.rev, got.at ?: kept.at, rule.version(def.kind), after)
            val wrote = try {
                files.keep(who, def.name, stands, got.bytes)
                true
            } catch (e: IOException) {
                false
            }
            if (got.outcome == SyncOutcome.TOOK && wrote) return SyncStep.Rest
            // The storage refused it: nothing was taken, and a fetch at once would only be refused again.
            if (got.outcome == SyncOutcome.TOOK) parts[def.name] = rule.after(part, SyncOutcome.FAILED, now())
            turn++
        }
    }

    private companion object {
        /** Fetches for one part at one look: "start again", then its whole. */
        const val TURNS = 3
        /** Covers' head, and a frame before its picture: the record, the picture's length. */
        const val COVERS_HEAD = 16
        const val COVER_FRAME = 12
        val NOTHING = IndexStands(null, null, null, SyncPart(whole = true, stopped = false, lost = 0u, failed = 0u, tried = null, started = 0L))
    }
}
