package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Where the music went at a handover, as a stop and a player's view name it (the owner, 2026-10-30). */
class MovedToTest {
    @Test
    fun a_handovers_stop_names_the_player_to_point_at_and_a_plain_stop_none() {
        assertEquals("player:Salon tablet", MovedTo.ofStop(JSONObject("""{"command":"stop","moved_to":"Salon tablet","version":7}""")))
        assertNull(MovedTo.ofStop(JSONObject("""{"command":"stop","version":7}""")))
        assertNull(MovedTo.ofStop(JSONObject("""{"command":"stop","moved_to":null}""")))
        assertNull(MovedTo.ofStop(JSONObject("""{"command":"stop","moved_to":"  "}""")))
    }

    @Test
    fun a_view_names_where_its_music_went_and_when() {
        val view = JSONObject("""{"name":"ThinkPad","moved_to":{"player":"Salon tablet","at":1761800000000}}""")
        assertEquals("player:Salon tablet" to 1_761_800_000_000L, MovedTo.ofView(view))
        assertNull(MovedTo.ofView(JSONObject("""{"name":"ThinkPad"}""")))
        assertNull(MovedTo.ofView(null))
    }
}
