package app.cratarr

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PageCommandTest {
    private fun op(op: String, vararg fields: Pair<String, Any?>): JSONObject {
        val m = JSONObject().put("type", "player").put("op", op)
        for ((k, v) in fields) m.put(k, v ?: JSONObject.NULL)
        return m
    }

    private fun keys(o: JSONObject): Set<String> = o.keys().asSequence().toSet()

    @Test
    fun a_queue_is_a_play_with_its_fields_in_cratarrs_shape() {
        val tracks = JSONArray().put(JSONObject().put("source", "local").put("id", "1"))
        val c = PageCommand.of(op("queue", "tracks" to tracks, "index" to 2, "positionMs" to 61_000L, "play" to true, "shuffled" to true, "name" to "Kind of Blue"))!!
        assertEquals("play", c.getString("command"))
        assertEquals(1, c.getJSONArray("tracks").length())
        assertEquals(2, c.getInt("index"))
        assertEquals(61_000L, c.getLong("position_ms"))
        assertFalse(c.getBoolean("paused"))
        assertTrue(c.getBoolean("shuffle"))
        assertEquals("Kind of Blue", c.getString("name"))
        assertEquals(setOf("command", "tracks", "index", "position_ms", "paused", "shuffle", "name"), keys(c))
    }

    @Test
    fun a_queue_not_to_play_is_loaded_paused_and_one_that_says_nothing_plays() {
        assertTrue(PageCommand.of(op("queue", "play" to false))!!.getBoolean("paused"))
        assertFalse(PageCommand.of(op("queue"))!!.getBoolean("paused"))
    }

    @Test
    fun a_play_after_a_mode_switch_loses_its_mark() {
        assertFalse(PageCommand.of(op("queue", "switched" to true))!!.has("switched"))
    }

    @Test
    fun play_is_resume() {
        val c = PageCommand.of(op("play"))!!
        assertEquals("resume", c.getString("command"))
        assertEquals(setOf("command"), keys(c))
    }

    @Test
    fun a_seek_takes_position_ms_and_keeps_its_wind() {
        val c = PageCommand.of(op("seek", "positionMs" to 30_500L, "wind" to true))!!
        assertEquals("seek", c.getString("command"))
        assertEquals(30_500L, c.getLong("position_ms"))
        assertTrue(c.getBoolean("wind"))
        assertEquals(setOf("command", "position_ms", "wind"), keys(c))
    }

    @Test
    fun a_sleep_timer_in_snake_case_its_cleared_time_still_null() {
        val set = PageCommand.of(op("sleep", "atMs" to 1_700_000_000_000L, "endOfTrack" to false))!!
        assertEquals("sleep", set.getString("command"))
        assertEquals(1_700_000_000_000L, set.getLong("at_ms"))
        assertFalse(set.getBoolean("end_of_track"))
        val end = PageCommand.of(op("sleep", "atMs" to null, "endOfTrack" to true))!!
        assertTrue(end.has("at_ms"))
        assertTrue(end.isNull("at_ms"))
        assertTrue(end.getBoolean("end_of_track"))
        assertEquals(setOf("command", "at_ms", "end_of_track"), keys(end))
    }

    @Test
    fun the_others_go_by_their_own_name_and_fields() {
        val add = PageCommand.of(op("add", "at" to 3, "after" to "e1", "tracks" to JSONArray()))!!
        assertEquals("add", add.getString("command"))
        assertEquals(3, add.getInt("at"))
        assertEquals("e1", add.getString("after"))
        val wind = PageCommand.of(op("wind", "dir" to "back", "phase" to "start"))!!
        assertEquals(setOf("command", "dir", "phase"), keys(wind))
        val toggle = PageCommand.of(op("toggle", "plain" to true))!!
        assertTrue(toggle.getBoolean("plain"))
        for (name in listOf("move", "remove", "upcoming", "jump", "next", "previous", "pause", "stop", "repeat", "radio", "autoplay", "skip_favourites", "direction")) {
            assertEquals(name, PageCommand.of(op(name))!!.getString("command"))
        }
    }

    @Test
    fun the_pages_own_and_unknown_calls_are_no_commands() {
        for (name in listOf("mode", "preview", "preview-end", "volume", "")) assertNull(name, PageCommand.of(op(name)))
    }

    @Test
    fun a_command_already_goes_as_it_is() {
        val m = JSONObject().put("type", "player").put("command", "seek").put("position_ms", 1_000L)
        val c = PageCommand.of(m)!!
        assertEquals(setOf("command", "position_ms"), keys(c))
        assertEquals(1_000L, c.getLong("position_ms"))
    }

    @Test
    fun the_message_itself_is_left_as_it_was() {
        val m = op("queue", "positionMs" to 5L, "play" to true)
        PageCommand.of(m)
        assertEquals("queue", m.getString("op"))
        assertEquals(5L, m.getLong("positionMs"))
    }
}
