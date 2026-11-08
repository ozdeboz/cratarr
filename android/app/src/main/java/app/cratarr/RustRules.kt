package app.cratarr

import android.content.Context
import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The players' rules from the shared Rust crate (server/rules, through UniFFI: server/rules-ffi), in place of this app's own
 * Kotlin, so the app, the page and the server keep one implementation. The library is in every build of the app (Gradle's
 * `rulesLibrary`). Only the brake so far ([Plan]). The Kotlin beside it serves the unit tests, which have no library, and a
 * device where it does not load, which says so once in the link log: the app ships no library for 32-bit ARM.
 */
object RustRules {
    /** The library loaded and answers as the Kotlin does. */
    @Volatile var on = false
        private set

    private val asked = AtomicBoolean(false)

    /** Loads the library, once a process, away from the caller's thread: JNA takes over 100 ms to bind it, a frame 16. */
    fun load(context: Context) {
        if (!asked.compareAndSet(false, true)) return
        val app = context.applicationContext
        Thread({ on = check(app) }, "cratarr-rules").apply { isDaemon = true }.start()
    }

    /** The library against the Kotlin over brakes like the tape's, in the link log. */
    private fun check(context: Context): Boolean = try {
        // The first call loads the library (JNA): timed apart from the rest.
        val loading = SystemClock.elapsedRealtimeNanos()
        app.cratarr.rules.brakeLengthMs(16.0, 1.0, 250.0)
        val loadMs = (SystemClock.elapsedRealtimeNanos() - loading) / 1e6
        var worst = 0.0
        var calls = 0
        for (v0 in listOf(-128.0, -16.0, 0.0, 0.5, 2.0, 16.0, 64.0, 128.0)) for (to in listOf(0.0, 1.0)) {
            val length = Plan.lengthMs(v0, to, 250L)
            val rust = app.cratarr.rules.brakeLengthMs(v0, to, 250.0).toLong()
            check(rust == length) { "length $v0→$to: Rust $rust, Kotlin $length" }
            val plan = Plan(0L, 0, 0L, 1, v0, to, length)
            for (t in 0L..length + 100L step 50L) {
                val d = length.toDouble()
                val tt = t.toDouble()
                worst = maxOf(
                    worst,
                    kotlin.math.abs(app.cratarr.rules.brakeV(v0, to, d, tt) - plan.kotlinV(t)),
                    kotlin.math.abs(app.cratarr.rules.brakeWay(v0, to, d, tt) - plan.kotlinWay(t)) / 1000.0,
                    kotlin.math.abs(app.cratarr.rules.brakeAccel(v0, to, d, tt) - plan.kotlinAccel(t)) / 1000.0,
                )
                calls += 3
            }
        }
        LinkLog.add(context, "info", "rules", "rust rules loaded", "calls" to calls, "worst" to worst, "load_ms" to loadMs)
        worst < 1e-9
    } catch (e: Throwable) {
        LinkLog.add(context, "warn", "rules", "rust rules not used", "error" to e.toString())
        false
    }
}
