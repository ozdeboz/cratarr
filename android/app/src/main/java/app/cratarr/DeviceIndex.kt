/*
 * The library on this device, kept up to date by the app (PREVIEW2's D15; the owner's Q3): so a car or a sleeping WebView still has
 * it, where a page in a browser keeps its own (web/src/lib/deviceSync). Whole once, then what changes. What to do, and what each
 * answer and each set of bytes came to, are the rule's (`index::sync` through UniFFI, the one the page asks), so this only fetches,
 * keeps and tells.
 * - What is kept is bytes (the owner's Q1), each account's apart ([Who]), hidden while another is signed in and there at its next
 *   sign-in: for each part a file of bytes and a small one of how it stands. Bytes first: a stop between the two leaves bytes ahead
 *   of their word, which a feed refuses, and a whole mends.
 * - Looked at a few seconds into a start (the app or its service opening, another account, hours away), on a new network, at the
 *   page's word, and every few hours by WorkManager, on any connection: the rule says what each may fetch.
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
import java.net.URLEncoder
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
        { fetched(app, it) },
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

    private fun fetched(app: Context, path: String): IndexFetched {
        val server = Prefs(app).serverUrl?.trimEnd('/') ?: return IndexFetched(null, null)
        return try {
            val c = Net.open(app, server + path)
            try {
                c.connectTimeout = 10_000
                c.readTimeout = 60_000
                val code = Net.code(app, c)
                IndexFetched(code, if (code in 200..299 && code != 204) c.inputStream.use { it.readBytes() } else null)
            } finally {
                c.disconnect()
            }
        } catch (e: IOException) {
            IndexFetched(null, null)
        }
    }

    /** WorkManager's every few hours. Not the kept items' worker, whose rules are the downloads': changes go on any connection. */
    fun schedule(context: Context) {
        val constraints = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
        val request = PeriodicWorkRequestBuilder<IndexWorker>(EVERY_HOURS, TimeUnit.HOURS).setConstraints(constraints).build()
        WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    private fun lookAt(app: Context, at: Long) {
        due?.cancel(false)
        due = worker.schedule({ lookHere(app) }, maxOf(0L, at - System.currentTimeMillis()), TimeUnit.MILLISECONDS)
    }

    private fun startHere(app: Context) {
        looks(app).startAnew()
        facts = emptyList()
        onChange?.invoke()
        lookAt(app, System.currentTimeMillis() + syncFirstAfterMs())
    }

    private fun lookHere(app: Context) {
        guarded {
            val server = Prefs(app).serverUrl
            val who = Who.current(app)
            if (server != null && who != Who.NONE && !Net.refused(app, server)) {
                val looks = looks(app)
                val steps = looks.look(who)
                facts = looks.facts(steps)
                if (inView) steps.values.filterIsInstance<SyncStep.Wait>().minOfOrNull { it.until }?.let { lookAt(app, it) }
                onChange?.invoke()
            }
        }
    }

    /** A start: another account's things are the device's, or the app or its service opened. */
    fun start(context: Context) {
        val app = context.applicationContext
        worker.execute { guarded { startHere(app) } }
    }

    /** Looks now: the page's word that the library moved, or a new network. */
    fun look(context: Context) {
        val app = context.applicationContext
        worker.execute { lookHere(app) }
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
        val known = facts.ifEmpty { INDEX_PARTS.mapNotNull { d -> files.stands(who, d.name)?.let { IndexFacts(it.part.whole, it.part.stopped, paused = false, asks = false) } } }
        return JSONObject()
            .put("type", "device-index")
            .put("at", index?.at ?: JSONObject.NULL)
            .put("rev", index?.rev ?: JSONObject.NULL)
            .put("metered", Net.metered(app))
            .put("parts", JSONArray(known.map { JSONObject().put("whole", it.whole).put("stopped", it.stopped).put("paused", it.paused).put("asks", it.asks) }))
    }

    /** The index as kept for the account signed in, for the page: its bytes, and when it last stood where Cratarr does. */
    fun answer(context: Context): WebResourceResponse {
        val app = context.applicationContext
        val who = Who.current(app)
        val files = files(app)
        val bytes = files.bytes(who, INDEX).takeIf { it.exists() }
        val stands = files.stands(who, INDEX)
        if (bytes == null || stands == null) return WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
        val headers = mapOf("Cache-Control" to "no-store", "X-Cratarr-At" to (stands.at ?: 0L).toString())
        return WebResourceResponse("application/octet-stream", null, 200, "OK", headers, FileInputStream(bytes))
    }
}

/** Every few hours: the library kept up to date, with no page open ([DeviceIndex.schedule]). */
class IndexWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result = try {
        DeviceIndex.syncAndWait(applicationContext)
        Result.success()
    } catch (e: Exception) {
        Result.retry()
    }
}

/** What a part is, to fetching it. D12's fits and D13's small covers are more rows here, as in lib/deviceSync. */
internal class IndexDef(val name: String, val kind: SyncKind, val whole: String, val feed: String)

internal val INDEX_PARTS = listOf(
    IndexDef("index", SyncKind.INDEX, "/api/index", "/api/index/changes"),
    IndexDef("sounds", SyncKind.SOUNDS, "/api/index/sounds", "/api/index/sounds/changes"),
)

/** What a fetch brought: its status (null: no answer at all), and its bytes where it had any. */
internal class IndexFetched(val status: Int?, val bytes: ByteArray?)

/** The rule, as the rules crate says it. In the unit tests, which have no library, a stand-in: all it proves there is what is done with its answers. */
internal interface IndexRule {
    fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, now: Long): SyncStep
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
    override fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, now: Long) = syncStep(part, network, mobile, now)
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
    private val fetch: (String) -> IndexFetched,
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

    /** Each part as far as it goes now, for [who]: what it was last told to do. */
    fun look(who: String): Map<String, SyncStep> = INDEX_PARTS.associate { it.name to bringUp(it, who) }

    /** The parts as this look left them. */
    fun facts(steps: Map<String, SyncStep>): List<IndexFacts> = INDEX_PARTS.mapNotNull { d ->
        parts[d.name]?.let { IndexFacts(it.whole, it.stopped, rule.paused(it), steps[d.name] == SyncStep.Ask) }
    }

    /** A part as this start takes it from what is kept. */
    private fun partOf(def: IndexDef, kept: IndexStands): SyncPart = parts.getOrPut(def.name) {
        val stood = kept.part.copy(started = started)
        if (kept.wants == rule.version(def.kind)) rule.anew(stood, started) else rule.anewByAnother(stood, started)
    }

    private fun whole(def: IndexDef): Got {
        val got = fetch(def.whole)
        rule.answer(got.status, true)?.let { return Got(it) }
        val bytes = got.bytes ?: return Got(SyncOutcome.LOST)
        val taken = rule.takeWhole(def.kind, bytes)
        return if (taken.outcome == SyncOutcome.TOOK) Got(SyncOutcome.TOOK, bytes, taken.rev, now()) else Got(taken.outcome)
    }

    private fun changes(def: IndexDef, who: String, kept: IndexStands): Got {
        val file = files.bytes(who, def.name)
        if (kept.rev == null || !file.exists()) return Got(SyncOutcome.START_AGAIN)
        val got = fetch("${def.feed}?since=${kept.rev}")
        rule.answer(got.status, false)?.let { return if (it == SyncOutcome.TOOK) Got(it, at = now()) else Got(it) }
        val feed = got.bytes ?: return Got(SyncOutcome.LOST)
        val taken = rule.takeChanges(def.kind, file.readBytes(), feed)
        return if (taken.outcome == SyncOutcome.TOOK) Got(SyncOutcome.TOOK, taken.bytes, taken.rev, now()) else Got(taken.outcome)
    }

    /** One part as far as it goes now: what it was last told to do. */
    private fun bringUp(def: IndexDef, who: String): SyncStep {
        var turn = 0
        while (true) {
            if (whoNow() != who) return SyncStep.Rest
            val kept = files.stands(who, def.name) ?: NOTHING
            val part = partOf(def, kept)
            val step = rule.step(part, network(), mobile(), now())
            if ((step != SyncStep.Whole && step != SyncStep.Changes) || turn == TURNS) return step
            val got = if (step == SyncStep.Whole) whole(def) else changes(def, who, kept)
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
        val NOTHING = IndexStands(null, null, null, SyncPart(whole = true, stopped = false, lost = 0u, failed = 0u, tried = null, started = 0L))
    }
}
