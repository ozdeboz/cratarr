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
    fun a_landing_past_a_tracks_real_end_is_the_next_tracks_start_or_the_queues_end() {
        // The tablet, 2026-10-28: Cratarr's map said 234.0 s, the file decodes to 233.586 s.
        assertEquals(Deck.Walk(2, 233_586L, true), Deck.settle(queue, 2, 234_000L))
        assertEquals(Deck.Walk(2, 0L, false), Deck.settle(queue, 1, 200_400L))
        assertEquals(Deck.Walk(1, 150_000L, false), Deck.settle(queue, 1, 150_000L))
    }
}
