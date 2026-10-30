package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

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
}
