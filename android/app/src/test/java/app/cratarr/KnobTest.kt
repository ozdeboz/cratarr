package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The direction knob and the holds against it ([Knob]), over the cases the page's web/tests/tape-knob.test.ts holds its
 * lib/player/knob to (0.12.4: pure directions, the owner 2026-10-31): the same answers on both.
 */
class KnobTest {
    @Test
    fun a_skip_plays_the_way_the_knob_is_set_the_knob_left_as_it_is() {
        // 0.10.1 put the knob at R on a skip; the owner (2026-10-31): only the user's turn changes it.
        assertEquals("at L: the new track backwards from its end", Knob.BACKWARDS, Knob.skipWay(Knob.L))
        assertEquals("at R: from its start", Knob.FORWARDS, Knob.skipWay(Knob.R))
    }

    @Test
    fun a_skip_at_L_stands_a_moment_before_the_tracks_end_and_plays_forwards_where_its_length_is_not_known() {
        assertEquals(238_500L, Knob.fromEnd(240_000L))
        assertNull("its length not known: forwards, the knob left at L", Knob.fromEnd(null))
        assertNull("too short to stand before its end", Knob.fromEnd(1_500L))
    }

    @Test
    fun holding_back_while_playing_backwards_let_go_backwards_again_at_1x() {
        // Let go: braked to 1× backwards, and it plays on that way.
        assertEquals(1, Knob.letGoTo(-1, true, Knob.L))
        assertEquals(Knob.BACKWARDS, Knob.afterLetGo(-1, true, Knob.L, "play"))
        // A pause pressed during the brake: it rests there, the knob still at L.
        assertEquals(Knob.REST, Knob.afterLetGo(-1, true, Knob.L, "pause"))
        // The knob turned to R while held: let go, it brakes to a stand and plays on forwards.
        assertEquals(0, Knob.letGoTo(-1, true, Knob.R))
        assertEquals(Knob.FORWARDS, Knob.afterLetGo(-1, true, Knob.R, "play"))
    }

    @Test
    fun the_knob_at_L_holding_forward_let_go_a_stand_then_backwards_at_1x() {
        // The owner (2026-10-31): "holding forward in order to wind to the right side switches the direction toggle on
        // the tape deck and doesn't switch it back". The setting stays at L throughout; the hold returns to it.
        assertEquals("braked to a stand (through zero)", 0, Knob.letGoTo(1, true, Knob.L))
        assertEquals("then backwards at 1×, the knob still at L", Knob.BACKWARDS, Knob.afterLetGo(1, true, Knob.L, "play"))
        assertEquals(Knob.REST, Knob.afterLetGo(1, true, Knob.L, "pause"))
        // The knob turned to R by the user during the hold: let go at 1× forwards.
        assertEquals(1, Knob.letGoTo(1, true, Knob.R))
        assertEquals(Knob.FORWARDS, Knob.afterLetGo(1, true, Knob.R, "play"))
    }

    @Test
    fun holding_forward_with_the_knob_at_R_let_go_at_1x_forwards_as_before() {
        assertEquals(1, Knob.letGoTo(1, false, Knob.R))
        assertEquals(Knob.FORWARDS, Knob.afterLetGo(1, false, Knob.R, "play"))
    }

    @Test
    fun a_hold_back_from_forwards_play_is_as_before_braked_to_a_stand_then_forwards() {
        assertEquals(0, Knob.letGoTo(-1, false, Knob.R))
        assertEquals(Knob.FORWARDS, Knob.afterLetGo(-1, false, Knob.R, "play"))
        // The knob at L but the hold not from backwards play (paused at L): a stand, as any hold back.
        assertEquals(0, Knob.letGoTo(-1, false, Knob.L))
    }
}
