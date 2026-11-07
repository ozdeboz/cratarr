package app.cratarr

import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import app.cratarr.rules.brakeLengthMs
import app.cratarr.rules.indexBytes
import app.cratarr.rules.indexBytesBack
import app.cratarr.rules.indexBytesLoads
import app.cratarr.rules.indexSpeed
import org.json.JSONObject
import java.io.File
import java.util.Locale
import kotlin.concurrent.thread

/**
 * The speed spike (preview 2's D03): the library's index over made-up tracks on this device, through UniFFI, its numbers against
 * the budgets, on the screen and in `files/index-speed.json`. It needs no sign-in, no server and no player, and all the loops lie
 * in Rust (server/rules-ffi/src/speed.rs). Only `-Pspike` turns it on (src/debug's manifest).
 *
 * `am start … --ei tracks 34000` runs it at once, so a device can be measured with nothing tapped.
 */
class SpeedActivity : AppCompatActivity() {
    private lateinit var run: Button
    private lateinit var said: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        run = Button(this).apply {
            text = "Run"
            setOnClickListener { measure(TRACKS) }
        }
        said = TextView(this).apply {
            typeface = Typeface.MONOSPACE
            textSize = 11f
            setTextIsSelectable(true)
            text = "$TRACKS made-up tracks"
        }
        val scroll = ScrollView(this).apply { addView(said) }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                fitsSystemWindows = true
                // Asleep halfway, a device's numbers are its sleep's.
                keepScreenOn = true
                setPadding(24, 24, 24, 24)
                addView(run)
                addView(scroll)
            },
        )
        val asked = intent.getIntExtra("tracks", 0)
        if (savedInstanceState == null && asked > 0) measure(asked)
    }

    private fun measure(tracks: Int) {
        run.isEnabled = false
        said.text = "Running over $tracks made-up tracks…"
        val file = File(filesDir, "index-speed.json")
        // A run that dies in Rust leaves no file, not the run before's numbers.
        file.delete()
        thread(name = "index-speed") {
            val shown = try {
                val out = numbers(tracks)
                file.writeText(out.toString(2))
                report(out)
            } catch (e: Throwable) {
                file.writeText(JSONObject().put("error", e.toString()).toString(2))
                "Did not run: $e"
            }
            runOnUiThread {
                said.text = shown
                run.isEnabled = true
            }
        }
    }

    /** Rust's numbers, and what only this side can say: the device, loading the library, handing bytes across. */
    private fun numbers(tracks: Int): JSONObject {
        val cold = !ran
        ran = true
        // The first call loads the library (JNA): timed apart, as RustRules does.
        val loading = SystemClock.elapsedRealtimeNanos()
        brakeLengthMs(16.0, 1.0, 250.0)
        val libraryMs = msSince(loading)
        val out = JSONObject(indexSpeed(tracks.toUInt()))
        val device = JSONObject()
            .put("model", Build.MODEL)
            .put("abi", Build.SUPPORTED_ABIS.first())
            .put("cores", Runtime.getRuntime().availableProcessors())
            .put("android", Build.VERSION.SDK_INT)
        return out
            .put("at", System.currentTimeMillis() / 1000)
            .put("device", device)
            .put("cold_process", cold)
            .put("library_ms", libraryMs)
            .put("handover", handover(tracks))
    }

    /**
     * The index out of Rust and back, each round sending on what came back: the app will hand 5 MB across where the brake only
     * ever sent numbers. Rust reads what is left at the end, and its sum says every byte of every crossing arrived.
     */
    private fun handover(tracks: Int): JSONObject {
        val bytes = indexBytes(tracks.toUInt())
        var back = bytes
        val took = DoubleArray(ROUNDS) {
            val start = SystemClock.elapsedRealtimeNanos()
            back = indexBytesBack(back)
            msSince(start)
        }
        took.sort()
        return JSONObject()
            .put("bytes", bytes.size)
            .put("rounds", ROUNDS)
            .put("median_ms", (took[ROUNDS / 2 - 1] + took[ROUNDS / 2]) / 2)
            .put("worst_ms", took.last())
            .put("same", back.contentEquals(bytes))
            .put("loads", indexBytesLoads(back))
    }

    /** `out` for the screen: the budgets first, as they are what it is for. */
    private fun report(out: JSONObject): String = buildString {
        if (out.has("refused")) {
            append("Refused: ").append(out.getString("refused"))
            return@buildString
        }
        val library = out.getJSONObject("library")
        val device = out.getJSONObject("device")
        val memory = out.getJSONObject("memory")
        val handover = out.getJSONObject("handover")
        val cold = out.getJSONObject("cold")
        appendLine("${library.getInt("tracks")} tracks, ${library.getInt("records")} records, ${library.getInt("heard")} heard")
        appendLine("${device.getString("model")}, ${device.getString("abi")}, Android ${device.getInt("android")}")
        appendLine("${out.getString("build")} build, ${if (out.getBoolean("cold_process")) "first run here" else "run again"}, library ${fixed(out.getDouble("library_ms"))} ms")
        appendLine()
        appendLine(line("Against the budgets, ms", "median", "worst", "budget", ""))
        val each = out.getJSONArray("each")
        for (at in 0 until each.length()) {
            val row = each.getJSONObject(at)
            val (median, worst, budget) = listOf("median_ms", "worst_ms", "budget_ms").map(row::getDouble)
            appendLine(line(row.getString("what"), fixed(median), fixed(worst), "%.0f".format(Locale.ROOT, budget), if (worst <= budget) "ok" else "OVER"))
        }
        appendLine()
        appendLine("From cold, ms")
        appendLine(line("to a crate", fixed(cold.getDouble("to_a_crate_ms"))))
        appendLine(line("to a Sounds like", fixed(cold.getDouble("to_a_sounds_like_ms"))))
        appendLine(line("to a pick", fixed(cold.getDouble("to_a_pick_ms"))))
        appendLine()
        appendLine(line("Once", "ms", "held MB"))
        val once = out.getJSONArray("once")
        for (at in 0 until once.length()) {
            val row = once.getJSONObject(at)
            appendLine(line(row.getString("what"), fixed(row.getDouble("ms")), fixed(row.getDouble("held_mb"))))
        }
        appendLine()
        appendLine(line("Memory, MB", "Rust", "resident"))
        for (name in listOf("before", "held", "after")) {
            val at = memory.getJSONObject(name)
            appendLine(line(name, fixed(at.getDouble("rust_mb")), fixed(at.optDouble("resident_mb"))))
        }
        appendLine(line("most, from the load on", fixed(memory.getDouble("most_rust_mb"))))
        appendLine()
        appendLine("${handover.getInt("bytes")} bytes there and back: median ${fixed(handover.getDouble("median_ms"))} ms, worst ${fixed(handover.getDouble("worst_ms"))} ms")
        appendLine("Came back the same: ${handover.getBoolean("same")}. Loads: ${handover.getBoolean("loads")}.")
    }

    /** A row in columns, narrow enough for a phone held upright. */
    private fun line(what: String, vararg rest: String): String = what.padEnd(25) + rest.joinToString("") { it.padStart(8) }

    private fun fixed(x: Double): String = "%.1f".format(Locale.ROOT, x)

    private fun msSince(start: Long): Double = (SystemClock.elapsedRealtimeNanos() - start) / 1e6

    private companion object {
        /** The real catalogue's size, near enough: 34,075. */
        const val TRACKS = 34_000
        const val ROUNDS = 20

        /** Whether this process has run it: the first run is the one with everything still to load. */
        @Volatile var ran = false
    }
}
