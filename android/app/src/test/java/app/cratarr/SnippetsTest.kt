package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** Track snippets (0.12, snippets: the owner chose them, 2026-10-30): the page's asks, the fades, the duck, the count. */
class SnippetsTest {
    private fun play(vararg fields: Pair<String, Any>) =
        JSONObject().put("type", "snippet").put("play", JSONObject().apply { fields.forEach { (k, v) -> put(k, v) } })

    @Test
    fun a_play_is_read_as_the_page_sends_it() {
        val m = play("source" to "cratarr", "id" to "tr-1", "from_ms" to 61_000, "to_ms" to 81_000, "fade_ms" to 500, "duck" to 0.2)
        assertEquals(SnippetAsk.Play("cratarr", "tr-1", 61_000L, 81_000L, 500L, 0.2f), SnippetAsk.of(m))
        assertEquals(20_000L, (SnippetAsk.of(m) as SnippetAsk.Play).lengthMs)
    }

    @Test
    fun a_stop_is_a_stop() {
        assertSame(SnippetAsk.Stop, SnippetAsk.of(JSONObject().put("type", "snippet").put("stop", true)))
    }

    @Test
    fun what_cannot_be_played_is_nothing() {
        assertNull(SnippetAsk.of(JSONObject().put("type", "snippet")))
        assertNull(SnippetAsk.of(play("id" to "tr-1", "from_ms" to 0, "to_ms" to 1_000)))
        assertNull(SnippetAsk.of(play("source" to "cratarr", "id" to "tr-1", "from_ms" to 5_000, "to_ms" to 5_000)))
        assertNull(SnippetAsk.of(play("source" to "cratarr", "id" to "tr-1", "from_ms" to 5_000)))
        assertNull(SnippetAsk.of(play("source" to "cratarr", "id" to JSONObject.NULL, "from_ms" to 0, "to_ms" to 1_000)))
    }

    @Test
    fun the_fades_and_duck_are_kept_in_bounds_and_default_when_missing() {
        val wild = SnippetAsk.of(play("source" to "cratarr", "id" to "tr-1", "from_ms" to -50, "to_ms" to 1_000, "fade_ms" to 5_000, "duck" to 3.0)) as SnippetAsk.Play
        assertEquals(0L, wild.fromMs)
        assertEquals(500L, wild.fadeMs)
        assertEquals(1f, wild.duck)
        val plain = SnippetAsk.of(play("source" to "cratarr", "id" to "tr-1", "from_ms" to 0, "to_ms" to 20_000)) as SnippetAsk.Play
        assertEquals(SnippetAsk.DEFAULT_FADE_MS, plain.fadeMs)
        assertEquals(SnippetAsk.DEFAULT_DUCK.toFloat(), plain.duck)
    }

    @Test
    fun the_snippet_fades_in_and_out_even_in_decibels() {
        // 20 s, 500 ms fades: silent at its ends, −30 dB halfway through a fade, full between.
        assertEquals(0f, SnippetCurve.gain(0L, 20_000L, 500L), 0f)
        assertEquals(0.0316f, SnippetCurve.gain(250L, 20_000L, 500L), 1e-4f)
        assertEquals(1f, SnippetCurve.gain(500L, 20_000L, 500L), 0f)
        assertEquals(1f, SnippetCurve.gain(10_000L, 20_000L, 500L), 0f)
        assertEquals(0.0316f, SnippetCurve.gain(19_750L, 20_000L, 500L), 1e-4f)
        assertEquals(0f, SnippetCurve.gain(20_000L, 20_000L, 500L), 0f)
        assertEquals(0f, SnippetCurve.gain(25_000L, 20_000L, 500L), 0f)
        // Rising throughout its fade in.
        val rise = (0..500 step 30).map { SnippetCurve.gain(it.toLong(), 20_000L, 500L) }
        assertTrue(rise.zipWithNext().all { (a, b) -> b > a })
        // No fade: full at once, silent at its end.
        assertEquals(1f, SnippetCurve.gain(0L, 20_000L, 0L), 0f)
        assertEquals(0f, SnippetCurve.gain(20_000L, 20_000L, 0L), 0f)
    }

    @Test
    fun the_duck_eases_even_in_decibels_and_lands_on_its_target() {
        // 1 → 0.2 over 400 ms: halfway in dB at the middle (√0.2), there at the end and after.
        assertEquals(1f, SnippetCurve.eased(1f, 0.2f, 0L, 400L), 0f)
        assertEquals(0.4472f, SnippetCurve.eased(1f, 0.2f, 200L, 400L), 1e-4f)
        assertEquals(0.2f, SnippetCurve.eased(1f, 0.2f, 400L, 400L), 0f)
        assertEquals(0.2f, SnippetCurve.eased(1f, 0.2f, 9_000L, 400L), 0f)
        // Slow at its ends, quickest in the middle (a raised cosine).
        val a = SnippetCurve.eased(1f, 0.2f, 40L, 400L)
        val b = SnippetCurve.eased(1f, 0.2f, 200L, 400L)
        val c = SnippetCurve.eased(1f, 0.2f, 240L, 400L)
        assertTrue(1f - a < b - c)
        // And back up the same way; a silent duck lands on silence, never past it.
        assertEquals(0.4472f, SnippetCurve.eased(0.2f, 1f, 200L, 400L), 1e-4f)
        assertEquals(0f, SnippetCurve.eased(1f, 0f, 400L, 400L), 0f)
        assertTrue(SnippetCurve.eased(1f, 0f, 399L, 400L) >= 0f)
        // No time to ease: at once.
        assertEquals(1f, SnippetCurve.eased(0.2f, 1f, 0L, 0L), 0f)
    }

    @Test
    fun the_clock_counts_only_while_it_sounds() {
        val c = SnippetClock(fromMs = 60_000L, lengthMs = 20_000L)
        assertFalse(c.started)
        assertEquals(0L, c.sounded(1_000L))
        c.playing(true, 1_000L)
        assertTrue(c.started)
        assertEquals(5_000L, c.sounded(6_000L))
        // A stall for the network: not counted.
        c.playing(false, 6_000L)
        assertEquals(5_000L, c.sounded(9_000L))
        c.playing(true, 9_000L)
        assertEquals(65_000L + 15_000L - 1L, c.atMs(23_999L))
        assertFalse(c.done(23_999L))
        assertTrue(c.done(24_000L))
        // Never past its stretch.
        assertEquals(80_000L, c.atMs(30_000L))
    }

    @Test
    fun the_page_hears_at_once_then_every_quarter_second_while_it_sounds() {
        val c = SnippetClock(fromMs = 60_000L, lengthMs = 20_000L, everyMs = 250L)
        // Not sounding yet: nothing to say.
        assertNull(c.tell(0L))
        c.playing(true, 100L)
        assertEquals(60_000L, c.tell(100L))
        // Stepped every 30 ms: told on the first step a quarter of a second on, not between.
        val told = (130L..1_100L step 30L).mapNotNull { t -> c.tell(t)?.let { t to it } }
        assertEquals(listOf(370L, 640L, 910L), told.map { it.first })
        assertEquals(listOf(60_270L, 60_540L, 60_810L), told.map { it.second })
        // Stalled: silent.
        c.playing(false, 1_100L)
        assertNull(c.tell(2_000L))
    }
}
