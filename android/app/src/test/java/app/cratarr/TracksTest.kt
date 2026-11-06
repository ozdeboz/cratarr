package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class TracksTest {
    private fun keys(o: JSONObject): Set<String> = o.keys().asSequence().toSet()

    @Test
    fun who_asked_stays_with_the_entry_kept_and_told_as_its_identity() {
        val track = JSONObject()
            .put("entry", "e-1").put("lane", "next").put("source", "cratarr").put("id", "tr-9")
            .put("name", "So What").put("artist", "Miles Davis").put("asked_by", "Guest Sam")
        // Kept with its item as text, read back as it was.
        val kept = JSONObject(track.toString())
        assertEquals("Guest Sam", kept.getString("asked_by"))
        val told = identityOf(kept)
        assertEquals(setOf("entry", "lane", "source", "id", "asked_by"), keys(told))
        assertEquals("Guest Sam", told.getString("asked_by"))
        // None asked: none said.
        assertFalse(identityOf(JSONObject().put("source", "cratarr").put("id", "tr-1")).has("asked_by"))
    }

    @Test
    fun a_tracks_copy_stays_with_its_entry_kept_and_told_as_its_identity() {
        val track = JSONObject().put("entry", "e-1").put("source", "cratarr").put("id", "tr-9").put("name", "So What").put("from", COPY)
        val told = identityOf(JSONObject(track.toString()))
        assertEquals(setOf("entry", "source", "id", "from"), keys(told))
        assertEquals(COPY, told.getString("from"))
        assertFalse(identityOf(JSONObject().put("source", "cratarr").put("id", "tr-1")).has("from"))
    }

    @Test
    fun a_track_names_a_copy_only_where_it_has_one() {
        assertEquals(COPY, copyNamed(JSONObject().put("from", COPY)))
        assertNull(copyNamed(JSONObject()))
        assertNull(copyNamed(JSONObject().put("from", "")))
        assertNull(copyNamed(JSONObject().put("from", JSONObject.NULL)))
        assertNull(copyNamed(null))
    }

    @Test
    fun a_copy_named_is_asked_of_the_file_its_id_whole_in_either_form() {
        assertEquals("http://nas/api/file/cratarr/tr-1?from=cratarr%2F%2Fnew%20music%2Fin", Downloads.fileUri("http://nas", "cratarr/tr-1", false, COPY))
        assertEquals("http://nas/api/file/cratarr/tr-1?as=opus&from=cratarr%2F%2Fnew%20music%2Fin", Downloads.fileUri("http://nas", "cratarr/tr-1", true, COPY))
    }

    @Test
    fun a_track_naming_no_copy_is_asked_of_the_file_as_ever() {
        assertEquals("http://nas/api/file/cratarr/tr-1", Downloads.fileUri("http://nas", "cratarr/tr-1", false))
        assertEquals("http://nas/api/file/cratarr/tr-1?as=opus", Downloads.fileUri("http://nas", "cratarr/tr-1", true))
    }

    /** Media items need Android's Bundle, which these tests have not: read from the source, which says a line is there. */
    private val downloads = File("src/main/java/app/cratarr/Downloads.kt").readText().replace("\r\n", "\n")

    @Test
    fun a_copy_named_goes_to_cratarr_before_anything_kept_is_looked_at() {
        val locate = downloads.substringAfter("fun locate(").substringBefore("private fun whole(")
        val named = locate.indexOf("copyOf(item)?.let")
        assertTrue("no branch for a copy named", named >= 0)
        assertTrue("a download is looked at first", named < locate.indexOf("downloadIndex"))
        assertTrue("the store is looked at first", named < locate.indexOf("whole(store"))
        assertFalse("kept under the track's name", locate.lines().first { "copyOf(item)?.let" in it }.contains("setCustomCacheKey"))
    }

    @Test
    fun a_copy_named_is_not_kept_ahead_under_the_tracks_name() {
        val wanted = downloads.lines().first { "val wanted = " in it }
        assertTrue(wanted, ".filter { copyOf(it) == null }" in wanted)
    }

    @Test
    fun a_wind_and_a_brake_ask_for_the_copy_their_track_is_heard_from() {
        val service = File("src/main/java/app/cratarr/PlayerService.kt").readText()
        assertEquals(2, service.split("\${windCopy(item)}\"").size - 1)
    }

    private companion object {
        /** A library's id is its folder, so it holds slashes, and here a space. */
        const val COPY = "cratarr//new music/in"
    }
}
