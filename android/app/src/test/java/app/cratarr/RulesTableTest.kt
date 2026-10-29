package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/** The app's player rules against the one table the server and the page check too (shared/rules.json; the audit,
 *  2026-10-29). */
class RulesTableTest {
    private val rules = JSONObject(File("../../shared/rules.json").readText())

    @Test
    fun theAppsRulesAreTheTables() {
        assertEquals(rules.getLong("restart_after_ms"), PlayerService.RESTART_AFTER_MS)
        assertEquals(rules.getLong("wind_cap_ms"), StartStop.WIND_CAP_MS)
        assertEquals(rules.getDouble("decel_x"), Plan.DECEL_X, 0.0)
        val tape = rules.getJSONObject("tape")
        val ranges = mapOf(
            "attackMs" to "attack_ms", "brakeMs" to "brake_ms", "gainDb" to "gain_db",
        ).mapValues { (_, k) -> tape.getJSONObject(k) } + mapOf(
            "toStart" to "point", "seekBack" to "point", "toEnd" to "point", "seekFwd" to "point", "holdBack" to "hold", "holdFwd" to "hold",
        ).mapValues { (_, k) -> tape.getJSONObject(k) }
        val kept = Prefs.TAPE_SETTINGS.values.flatMap { it.entries }.associate { it.key to it.value }
        assertEquals(ranges.keys, kept.keys)
        for ((name, r) in kept) {
            val want = ranges.getValue(name)
            assertEquals(name, want.getDouble("min"), r.min.toDouble(), 1e-6)
            assertEquals(name, want.getDouble("max"), r.max.toDouble(), 1e-6)
            assertEquals(name, want.getDouble("start"), r.start.toDouble(), 1e-6)
        }
    }

    @Test
    fun aBrakeIsTheTables() {
        val brakes = rules.getJSONArray("brake")
        for (i in 0 until brakes.length()) {
            val b = brakes.getJSONObject(i)
            val (v0, to) = b.getDouble("v0") to b.getDouble("to")
            val length = Plan.lengthMs(v0, to, b.getLong("brake_ms"))
            assertEquals("brake $i", b.getLong("length_ms"), length)
            val plan = Plan(t0 = 0L, index = 0, ms = 0L, sign = 1, v0 = v0, to = to, brakeMs = length)
            val at = b.getJSONArray("at")
            for (j in 0 until at.length()) {
                val p = at.getJSONObject(j)
                val t = p.getLong("t_ms")
                assertEquals("brake $i at $t: speed", p.getDouble("v"), plan.v(t), 1e-9)
                assertEquals("brake $i at $t: way", p.getDouble("way_ms"), plan.way(t), 1e-6)
            }
        }
    }
}
