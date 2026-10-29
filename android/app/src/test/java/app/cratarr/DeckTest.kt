package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Test

class DeckTest {
    private val queue = listOf(120_000L, 200_000L, 233_586L)

    @Test
    fun a_walk_forward_goes_on_into_the_next_tracks() {
        assertEquals(Deck.Walk(1, 30_000L, false), Deck.walk(queue, 0, 100_000L, 50_000L))
        assertEquals(Deck.Walk(2, 10_000L, false), Deck.walk(queue, 0, 100_000L, 230_000L))
    }

    @Test
    fun a_walk_back_goes_into_the_tracks_before() {
        assertEquals(Deck.Walk(0, 110_000L, false), Deck.walk(queue, 1, 5_000L, -15_000L))
    }

    @Test
    fun a_walk_stops_at_the_queues_ends() {
        assertEquals(Deck.Walk(2, 233_586L, true), Deck.walk(queue, 2, 233_000L, 10_000L))
        assertEquals(Deck.Walk(0, 0L, true), Deck.walk(queue, 1, 5_000L, -500_000L))
    }

    @Test
    fun an_unknown_length_is_taken_from_the_fallback() {
        assertEquals(Deck.Walk(1, 1_000L, false), Deck.walk(listOf(0L, 50_000L), 0, 0L, 11_000L) { 10_000L })
    }

    @Test
    fun a_plan_brakes_on_a_raised_cosine_its_way_the_mean_of_its_ends() {
        val p = Plan(t0 = 0L, index = 0, ms = 0L, sign = 1, v0 = 64.0, to = 0.0, brakeMs = 1_500L)
        assertEquals(64.0, p.v(0L), 1e-9)
        assertEquals(32.0, p.v(750L), 1e-9)
        assertEquals(0.0, p.v(1_500L), 1e-9)
        assertEquals(32.0 * 1_500.0, p.way(1_500L), 1e-6)
        assertEquals(32.0 * 1_500.0, p.way(9_000L), 1e-6)
        // Its change: steepest halfway, the speed's slope there.
        assertEquals(-64.0 * Math.PI / 3.0, p.accel(750L), 1e-6)
        assertEquals((p.v(751L) - p.v(749L)) / 2.0 * 1000.0, p.accel(750L), 0.01)
    }

    @Test
    fun a_brake_takes_longer_from_a_higher_speed_never_shorter_than_the_brake_set() {
        assertEquals(2_000L, Plan.lengthMs(129.0, 1.0, 250L))
        assertEquals(500L, Plan.lengthMs(33.0, 1.0, 250L))
        // Slow: the Brake as set.
        assertEquals(250L, Plan.lengthMs(8.0, 1.0, 250L))
        assertEquals(400L, Plan.lengthMs(1.0, 0.0, 400L))
        // Backwards, to still.
        assertEquals(1_000L, Plan.lengthMs(64.0, 0.0, 250L))
    }

    @Test
    fun a_landing_past_a_tracks_real_end_is_the_next_tracks_start_or_the_queues_end() {
        // The tablet, 2026-10-28: Cratarr's map said 234.0 s, the file decodes to 233.586 s.
        assertEquals(Deck.Walk(2, 233_586L, true), Deck.settle(queue, 2, 234_000L))
        assertEquals(Deck.Walk(2, 0L, false), Deck.settle(queue, 1, 200_400L))
        assertEquals(Deck.Walk(1, 150_000L, false), Deck.settle(queue, 1, 150_000L))
    }
}
