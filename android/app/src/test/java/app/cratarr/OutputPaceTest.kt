package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The output's speed as it ran (2026-11-03, the owner: "we need to measure the tape truthfully. No guessing anywhere"):
 * a place last told is counted on through each speed it had since, never at the one set now for the time before.
 */
class OutputPaceTest {
    private val ms = 1_000_000L

    @Test
    fun a_place_is_counted_on_through_each_speed_it_ran_at() {
        val p = OutputPace()
        p.set(0, 1.0)
        // A brake from 1× to a quarter 600 ms after the last word: the 600 ms before it went at 1×.
        p.set(600 * ms, 0.25)
        assertEquals(600_000.0 + 400_000.0 * 0.25, p.goneUs(0, 1_000 * ms), 1e-6)
        // From a word in the middle of the first stretch.
        assertEquals(300_000.0 + 200_000.0 * 0.25, p.goneUs(300 * ms, 800 * ms), 1e-6)
    }

    @Test
    fun a_brake_never_counts_the_past_slower_than_it_went() {
        // The tablet's let-go from −1×: a word, then the speed set lower every 16 ms. Counted the old way (the time since
        // the word at the speed now) the tape fell behind its plan and was steered faster; counted truly, the way only
        // grows by what each stretch played.
        val p = OutputPace()
        p.set(0, 1.0)
        var truth = 0.0
        var speed = 1.0
        var at = 0L
        for (step in 1..30) {
            val t = (400 + 16 * step) * ms
            truth += (t - at) / 1000.0 * speed
            at = t
            speed = maxOf(0.25, 1.0 - step / 30.0)
            p.set(t, speed)
            assertEquals(truth, p.goneUs(0, t), 1e-6)
            // The old way's error: never below the truth here.
            assertEquals(true, p.goneUs(0, t) >= t / 1000.0 * speed)
        }
    }

    @Test
    fun paused_it_stays_and_before_any_speed_it_is_still() {
        val p = OutputPace()
        assertEquals(0.0, p.goneUs(0, 500 * ms), 0.0)
        p.set(100 * ms, 1.0)
        p.set(300 * ms, 0.0)
        p.set(700 * ms, 1.0)
        assertEquals(200_000.0 + 100_000.0, p.goneUs(0, 800 * ms), 1e-6)
        assertEquals(0.0, p.at(50 * ms), 0.0)
        assertEquals(0.0, p.at(500 * ms), 0.0)
        assertEquals(1.0, p.at(800 * ms), 0.0)
    }

    @Test
    fun what_is_forgotten_is_only_what_no_word_counts_from_again() {
        val p = OutputPace()
        p.set(0, 1.0)
        p.set(100 * ms, 0.5)
        p.set(200 * ms, 2.0)
        p.forget(150 * ms)
        // From a word at 150 ms: the half speed then is still known.
        assertEquals(50_000.0 * 0.5 + 100_000.0 * 2.0, p.goneUs(150 * ms, 300 * ms), 1e-6)
    }
}
