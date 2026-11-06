package app.cratarr

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.IOException

/** The kept lists on the device: a stop or a full disk in a write must not read as nothing kept, or the next sync deletes it all. */
class KeptFileTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val list = """[{"kind":"album","source":"cratarr","id":"al-1"},{"kind":"crate","source":"cratarr","id":"cr-2"}]"""
    private val longer = """[{"kind":"album","source":"cratarr","id":"al-1"},{"kind":"crate","source":"cratarr","id":"cr-2"},{"kind":"artist","source":"cratarr","id":"ar-3"}]"""

    private fun kept() = File(folder.root, "kept-items.json")

    @Test
    fun a_write_cut_off_leaves_the_last_list_whole() {
        val file = kept()
        KeptFile.write(file, list)
        assertThrows(IOException::class.java) {
            KeptFile.write(file, longer) { to, text ->
                to.writeText(text.take(text.length / 2))
                throw IOException("no room left")
            }
        }
        assertEquals(list, file.readText())
        assertEquals(2, KeptFile.list(file).size)
    }

    @Test
    fun a_write_that_cannot_begin_leaves_the_last_list_whole() {
        val file = kept()
        KeptFile.write(file, list)
        assertThrows(IOException::class.java) { KeptFile.write(file, longer) { _, _ -> throw IOException("no room left") } }
        assertEquals(list, file.readText())
    }

    @Test
    fun a_write_takes_the_lists_place_and_leaves_nothing_beside_it() {
        val file = kept()
        KeptFile.write(file, list)
        KeptFile.write(file, longer)
        assertEquals(longer, file.readText())
        assertEquals(listOf(file.name), folder.root.list()!!.toList())
    }

    @Test
    fun what_a_stop_left_aside_is_neither_read_nor_in_the_way() {
        val file = kept()
        KeptFile.write(file, list)
        val aside = File(folder.root, "${file.name}.new")
        aside.writeText(longer.take(20))
        assertEquals(2, KeptFile.list(file).size)
        KeptFile.write(file, longer)
        assertEquals(3, KeptFile.list(file).size)
        assertTrue(!aside.exists())
    }

    @Test
    fun a_list_cut_off_is_not_read_as_nothing_kept() {
        val file = kept()
        file.writeText(list.take(list.length / 2))
        assertThrows(IOException::class.java) { KeptFile.list(file) }
    }

    @Test
    fun a_list_emptied_is_not_read_as_nothing_kept() {
        val file = kept()
        file.writeText("")
        assertThrows(IOException::class.java) { KeptFile.list(file) }
    }

    @Test
    fun a_list_of_something_else_is_not_read_as_nothing_kept() {
        val file = kept()
        file.writeText("""{"kind":"album"}""")
        assertThrows(IOException::class.java) { KeptFile.list(file) }
        file.writeText("""[{"kind":"album"},"cut"]""")
        assertThrows(IOException::class.java) { KeptFile.list(file) }
    }

    private fun holders() = File(folder.root, "kept-holders.json")

    @Test
    fun the_holders_read_as_they_were_written() {
        val file = holders()
        KeptFile.write(file, JSONObject().put("cratarr/tr-1", JSONArray(listOf("@ann\tmanual", "@bo\talbum:cratarr/al-1"))).toString())
        assertEquals(mapOf("cratarr/tr-1" to setOf("@ann\tmanual", "@bo\talbum:cratarr/al-1")), KeptFile.holders(file))
    }

    @Test
    fun holders_cut_off_are_not_read_as_nothing_held() {
        // Nothing held, one account's Remove would take every account's downloads.
        val file = holders()
        file.writeText("""{"cratarr/tr-1":["@ann\tman""")
        assertThrows(IOException::class.java) { KeptFile.holders(file) }
        file.writeText("")
        assertThrows(IOException::class.java) { KeptFile.holders(file) }
        file.writeText("[]")
        assertThrows(IOException::class.java) { KeptFile.holders(file) }
    }

    @Test
    fun no_holders_yet_are_none() {
        assertEquals(emptyMap<String, Set<String>>(), KeptFile.holders(holders()))
    }

    @Test
    fun a_holder_of_no_one_is_not_read() {
        val file = holders()
        file.writeText("""{"cratarr/tr-1":["manual"]}""")
        assertEquals(mapOf("cratarr/tr-1" to emptySet<String>()), KeptFile.holders(file))
    }

    @Test
    fun no_list_yet_is_nothing_kept() {
        assertEquals(emptyList<Any>(), KeptFile.list(kept()))
    }

    @Test
    fun a_list_written_is_read_back() {
        val file = kept()
        KeptFile.write(file, list)
        assertEquals(listOf("al-1", "cr-2"), KeptFile.list(file).map { it.optString("id") })
    }

    @Test
    fun a_list_saved_empty_is_nothing_kept() {
        val file = kept()
        KeptFile.write(file, "[]")
        assertEquals(emptyList<Any>(), KeptFile.list(file))
    }
}
