package app.cratarr

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.ComponentCallbacks2
import android.content.Context
import android.os.Build
import android.os.Debug
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import kotlin.math.abs

/**
 * What the device does to the app, in the app's own log ([LinkLog]), so that it reaches Cratarr with the rest: why each run
 * ended, as Android tells it, and the memory the app holds, its graphics too, on which page. The owner, 2026-11-06: a
 * tablet's memory cleaner stopped the music for days and its page held 732 MB, and only adb could tell.
 */
object DeviceWatch {
    private const val TAG = "CratarrDevice"
    private const val AREA = "device"
    /** How often the memory is looked at: reading it costs next to nothing, and most looks write no line. */
    private const val SAMPLE_MS = 60_000L
    /** A line at least this often, however still the memory is. */
    private const val REGULAR_MS = 30 * 60_000L
    /** A change this large is a line at once. */
    private const val STEP_KB = 64 * 1024L
    /** The most ends of runs asked of Android at once. */
    private const val EXITS = 16
    private const val WHY_CHARS = 200

    /** The page open, by its route, as the WebView tells it; null: none yet. */
    @Volatile var page: String? = null

    private var watch: ScheduledExecutorService? = null
    private var loggedKb: Long? = null
    private var loggedAt = 0L

    /** Watching from now on: why the runs before this one ended, then the memory. */
    fun start(context: Context) {
        val app = context.applicationContext
        synchronized(this) {
            if (watch != null) return
            watch = Executors.newSingleThreadScheduledExecutor { r -> Thread(r, "device-watch").apply { isDaemon = true } }.also {
                it.execute { guarded { runsEnded(app) } }
                it.scheduleWithFixedDelay({ guarded { sample(app, null) } }, 0, SAMPLE_MS, TimeUnit.MILLISECONDS)
            }
        }
    }

    fun stop() {
        synchronized(this) {
            watch?.shutdownNow()
            watch = null
        }
    }

    /** Android asks the app to give memory back: a line, at once, with what it holds. */
    fun trimmed(context: Context, level: Int) {
        val word = trimWord(level) ?: return
        val app = context.applicationContext
        synchronized(this) { runCatching { watch?.execute { guarded { sample(app, word) } } } }
    }

    /** The page's renderer is gone: by a crash of its own, or ended by Android for its memory. */
    fun rendererGone(context: Context, crashed: Boolean, priority: Int) =
        LinkLog.add(context, "warn", AREA, "page renderer gone", "crashed" to crashed, "priority" to priority, "page" to page)

    private inline fun guarded(block: () -> Unit) {
        try {
            block()
        } catch (e: Exception) {
            Log.w(TAG, "cannot read the device: ${e.message}")
        }
    }

    /** Each run's end Android holds and this log does not yet, oldest first. A run's pages have processes of their own: left out. */
    private fun runsEnded(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val prefs = Prefs(context)
        val seen = prefs.exitsSeen
        val ends = context.getSystemService(ActivityManager::class.java)
            .getHistoricalProcessExitReasons(context.packageName, 0, EXITS)
            .filter { it.processName == context.packageName && it.timestamp > seen }
            .sortedBy { it.timestamp }
        for (e in ends) {
            LinkLog.add(
                context,
                if (quietEnd(e.reason)) "info" else "warn",
                AREA,
                "run ended",
                "reason" to reasonWord(e.reason),
                "why" to e.description?.take(WHY_CHARS),
                "ended_at" to e.timestamp,
                "importance" to e.importance,
                "rss_mb" to mb(e.rss),
                "pss_mb" to mb(e.pss),
            )
        }
        ends.lastOrNull()?.let { prefs.exitsSeen = it.timestamp }
    }

    /** A line when one is [due], or [trim] says Android asked for memory back. */
    private fun sample(context: Context, trim: String?) {
        val own = read("/proc/self/status") ?: return
        val rss = kbOf(own, "VmRSS") ?: return
        val now = SystemClock.elapsedRealtime()
        val why = trim ?: due(loggedKb, rss, now - loggedAt) ?: return
        loggedKb = rss
        loggedAt = now
        val am = context.getSystemService(ActivityManager::class.java)
        val stats = Debug.MemoryInfo().also(Debug::getMemoryInfo)
        val system = ActivityManager.MemoryInfo().also(am::getMemoryInfo)
        val state = ActivityManager.RunningAppProcessInfo().also(ActivityManager::getMyMemoryState)
        LinkLog.add(
            context,
            "info",
            AREA,
            "memory",
            "why" to why,
            "page" to page,
            "importance" to state.importance,
            "rss_mb" to mb(rss),
            "anon_mb" to kbOf(own, "RssAnon")?.let(::mb),
            "file_mb" to kbOf(own, "RssFile")?.let(::mb),
            "shared_mb" to kbOf(own, "RssShmem")?.let(::mb),
            "swap_mb" to kbOf(own, "VmSwap")?.let(::mb),
            "graphics_mb" to stat(stats, "summary.graphics"),
            "java_mb" to stat(stats, "summary.java-heap"),
            "native_mb" to stat(stats, "summary.native-heap"),
            "code_mb" to stat(stats, "summary.code"),
            "pss_mb" to stat(stats, "summary.total-pss"),
            "device_free_mb" to system.availMem / (1024 * 1024),
            "device_low" to system.lowMemory,
            "device_swap_free_mb" to read("/proc/meminfo")?.let { kbOf(it, "SwapFree") }?.let(::mb),
        )
    }

    private fun stat(m: Debug.MemoryInfo, name: String): Long? = m.getMemoryStat(name)?.toLongOrNull()?.let(::mb)

    private fun mb(kb: Long): Long = kb / 1024

    private fun mb(kb: Int): Long = mb(kb.toLong())

    private fun read(path: String): String? = runCatching { File(path).readText() }.getOrNull()

    /** A value in kB from `/proc`'s status or meminfo, by its name. */
    fun kbOf(text: String, key: String): Long? =
        text.lineSequence().firstOrNull { it.startsWith("$key:") }?.substringAfter(':')?.trim()?.substringBefore(' ')?.toLongOrNull()

    /** Why a line is due, for [kb] now against the last line's and [sinceMs] after it; null: none is. */
    fun due(lastKb: Long?, kb: Long, sinceMs: Long): String? = when {
        lastKb == null -> "first"
        abs(kb - lastKb) >= STEP_KB -> "changed"
        sinceMs >= REGULAR_MS -> "regular"
        else -> null
    }

    /** The page's route in its address: `#/crates/7?a=b` is `crates`, what a record's or an artist's route holds left out. */
    fun pageOf(url: String): String? =
        url.substringAfter('#', "").trimStart('/').substringBefore('?').substringBefore('/').take(32).ifEmpty { null }

    /** Android's asking for memory back, in a word; null: not worth a line (the app merely going down the list). */
    @Suppress("DEPRECATION")
    fun trimWord(level: Int): String? = when (level) {
        ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW -> "running low"
        ComponentCallbacks2.TRIM_MEMORY_RUNNING_CRITICAL -> "running critical"
        ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN -> "page left"
        ComponentCallbacks2.TRIM_MEMORY_MODERATE -> "background moderate"
        ComponentCallbacks2.TRIM_MEMORY_COMPLETE -> "background complete"
        else -> null
    }

    /** An end that says nothing went wrong. */
    fun quietEnd(reason: Int): Boolean = reason == ApplicationExitInfo.REASON_EXIT_SELF ||
        reason == ApplicationExitInfo.REASON_OTHER ||
        reason == ApplicationExitInfo.REASON_PACKAGE_UPDATED ||
        reason == ApplicationExitInfo.REASON_PACKAGE_STATE_CHANGE

    fun reasonWord(reason: Int): String = when (reason) {
        ApplicationExitInfo.REASON_EXIT_SELF -> "exit self"
        ApplicationExitInfo.REASON_SIGNALED -> "signaled"
        ApplicationExitInfo.REASON_LOW_MEMORY -> "low memory"
        ApplicationExitInfo.REASON_CRASH -> "crash"
        ApplicationExitInfo.REASON_CRASH_NATIVE -> "native crash"
        ApplicationExitInfo.REASON_ANR -> "not responding"
        ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "initialization failure"
        ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "permission change"
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "excessive resource usage"
        ApplicationExitInfo.REASON_USER_REQUESTED -> "user requested"
        ApplicationExitInfo.REASON_USER_STOPPED -> "user stopped"
        ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "dependency died"
        ApplicationExitInfo.REASON_OTHER -> "other"
        ApplicationExitInfo.REASON_FREEZER -> "freezer"
        ApplicationExitInfo.REASON_PACKAGE_STATE_CHANGE -> "package state change"
        ApplicationExitInfo.REASON_PACKAGE_UPDATED -> "package updated"
        else -> "unknown $reason"
    }

    /**
     * Music playing while the player's service is out of the foreground, where Android may stop it at any moment: told once
     * a spell, past [graceMs], the time Media3 takes to put its notification up.
     */
    class Unprotected(private val graceMs: Long) {
        private var since: Long? = null
        private var told = false

        /** A look while it plays; true: tell of it now. */
        fun playing(foreground: Boolean, now: Long): Boolean {
            if (foreground) {
                stopped()
                return false
            }
            val from = since ?: now.also { since = it }
            if (told || now - from < graceMs) return false
            told = true
            return true
        }

        /** It stopped playing, or is in the foreground: the next spell is told of anew. */
        fun stopped() {
            since = null
            told = false
        }
    }
}
