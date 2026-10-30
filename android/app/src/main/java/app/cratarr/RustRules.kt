package app.cratarr

import android.content.Context
import android.os.SystemClock

/**
 * Spike B (0.11; the owner, 2026-10-30): the players' rules from the shared Rust crate (server/rules, through UniFFI:
 * server/rules-ffi), in place of this app's own Kotlin, so the app, the page and the server keep one implementation.
 * Behind the device's `rustRules` switch, off by default. An app built without the library
 * (scripts/rules-android.sh) or a device where it does not load keeps the Kotlin, and says so once in the link log.
 * Only the brake so far ([Plan]).
 */
object RustRules {
    /** The switch, as set ([Prefs.rustRules]). */
    @Volatile var wanted = false

    /** The library loaded and answered; null until first asked. */
    @Volatile private var loaded: Boolean? = null

    /** Rust's rules are used: switched on, and the library is here. */
    val on: Boolean get() = wanted && (loaded ?: false)

    /** Takes the switch; switched on, loads the library once and compares it with the Kotlin (in the link log). */
    fun set(context: Context, on: Boolean) {
        wanted = on
        if (on && loaded == null) loaded = check(context)
    }

    /** The library against the Kotlin over brakes like the tape's, and how long a call takes. */
    private fun check(context: Context): Boolean = try {
        // The first call loads the library (JNA): timed apart from a warm call.
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
            for (t in 0L..length + 100L step 10L) {
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
        // A warm call against the Kotlin: as a brake's frame asks.
        var sink = 0.0
        val n = 20_000
        val rustStart = SystemClock.elapsedRealtimeNanos()
        for (i in 0 until n) sink += app.cratarr.rules.brakeWay(64.0, 1.0, 1000.0, (i % 1000).toDouble())
        val rustNs = (SystemClock.elapsedRealtimeNanos() - rustStart).toDouble() / n
        val plan = Plan(0L, 0, 0L, 1, 64.0, 1.0, 1000L)
        val kotlinStart = SystemClock.elapsedRealtimeNanos()
        for (i in 0 until n) sink += plan.kotlinWay((i % 1000).toLong())
        val kotlinNs = (SystemClock.elapsedRealtimeNanos() - kotlinStart).toDouble() / n
        LinkLog.add(
            context, "info", "rules", "rust rules loaded",
            "calls" to calls, "worst" to worst, "load_ms" to loadMs, "rust_ns" to rustNs, "kotlin_ns" to kotlinNs, "sink" to sink,
        )
        worst < 1e-9
    } catch (e: Throwable) {
        LinkLog.add(context, "warn", "rules", "rust rules not used", "error" to e.toString())
        false
    }
}
