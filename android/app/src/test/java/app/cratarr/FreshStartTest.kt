package app.cratarr

import app.cratarr.rules.FreshFacts
import app.cratarr.rules.FreshStep
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * 0.16's one fresh start (decision 0101 point 23) as the app does it ([FreshLooks]), on a stand-in device and a stand-in of the rule
 * (the rules crate's own tests prove the rule; this proves what is done with its answers): what is written down, what goes, what
 * comes again and for whom, and that a stop at any step loses nothing.
 */
class FreshStartTest {
    @get:Rule val dir = TemporaryFolder()

    private val a = "@anna"
    private val b = "@ben"
    private val now = 1600

    /** The rules crate's `fresh::step`, as it reads. */
    private val rule = object : FreshRule {
        override fun step(facts: FreshFacts, version: Int): FreshStep {
            val due = version >= 1600 && (facts.marked == null || facts.marked!! < 1600u)
            if (due) {
                if (!facts.collected && !facts.keptReadable) return FreshStep.NOTHING
                return if (!facts.collected) FreshStep.COLLECT else if (!facts.cleared) FreshStep.CLEAR else FreshStep.MARK
            }
            return if (facts.waitingHere && facts.reached && facts.mayDownload && facts.keptReadable) FreshStep.FETCH else FreshStep.NOTHING
        }
    }

    private fun track(id: String, album: String) = JSONObject().put("source", "cratarr").put("id", id).put("album_id", album).put("name", id).put("added", 1L)

    /** A device as 0.15 left it: Anna's two downloads and a kept album, Ben's one download (shared with Anna) and a kept playlist. */
    private inner class Device : FreshDevice {
        var marked: Int? = null
        var items = mutableListOf(
            JSONObject().put("who", a).put("kind", "album").put("source", "cratarr").put("id", "al-old1").put("name", "One"),
            JSONObject().put("who", b).put("kind", "playlist").put("source", "cratarr").put("id", "Mix").put("name", "Mix"),
        )
        var holders = mutableMapOf(
            "cratarr/tr-old1" to setOf(KeptHolds.held(a, Kept.MANUAL)),
            "cratarr/tr-old2" to setOf(KeptHolds.held(a, Kept.MANUAL), KeptHolds.held(b, Kept.MANUAL)),
            "cratarr/tr-old3" to setOf(KeptHolds.held(a, "album:cratarr/al-old1")),
        )
        var stored = mutableMapOf(
            "cratarr/tr-old1" to track("tr-old1", "al-old1"),
            "cratarr/tr-old2" to track("tr-old2", "al-old2"),
            "cratarr/tr-old3" to track("tr-old3", "al-old1"),
        )
        var readable = true
        var who = a
        var reached = true
        var may = true
        var clearsLeft = Int.MAX_VALUE
        var snapshots = 0
        val downloaded = mutableListOf<Pair<String, JSONObject>>()
        val keptAgain = mutableListOf<String>()
        val movedAsked = mutableListOf<List<String>>()

        override fun marked() = marked
        override fun mark(version: Int) {
            marked = version
        }
        override fun snapshot(): Pair<List<JSONObject>, Map<String, Set<String>>>? {
            snapshots++
            return if (readable) items.toList() to holders.toMap() else null
        }
        override fun downloads() = stored.toMap()
        override fun clear(): Boolean {
            if (clearsLeft-- <= 0) return false
            items.clear()
            holders.clear()
            stored.clear()
            return true
        }
        override fun who() = who
        override fun reached() = reached
        override fun mayDownload() = may
        override fun moved(ids: List<String>): Map<String, String>? {
            if (!reached) return null
            movedAsked.add(ids)
            return mapOf("tr-old1" to "tr-1", "tr-old2" to "tr-2", "al-old1" to "al-1", "al-old2" to "al-2").filterKeys { it in ids }
        }
        override fun playlistsMoved() = if (reached) mapOf("Mix" to "pl-7") else null
        override fun download(tracks: JSONArray) {
            for (i in 0 until tracks.length()) downloaded.add(who to tracks.getJSONObject(i))
        }
        override fun keep(kind: String, source: String, id: String, name: String) {
            keptAgain.add("$who $kind:$source/$id $name")
        }
    }

    private lateinit var device: Device
    private lateinit var file: File

    @Before
    fun setUp() {
        device = Device()
        file = File(dir.root, "fresh-0.16.json")
    }

    private fun look(version: Int = now) = FreshLooks(rule, device, file, version).look()

    private fun waiting(): Set<String> = JSONObject(file.readText()).getJSONObject("accounts").keys().asSequence().toSet()

    @Test
    fun an_upgrade_writes_down_each_accounts_own_clears_marks_and_fetches_the_signed_in_ones_under_the_ids_they_have_now() {
        look()
        assertEquals(now, device.marked)
        assertTrue("the stores of before are gone", device.stored.isEmpty() && device.items.isEmpty())
        assertEquals(
            "Anna's own downloads, under their new ids; the kept album's track comes with the album",
            listOf("tr-1/al-1", "tr-2/al-2"),
            device.downloaded.map { (who, t) -> assertEquals(a, who); "${t.getString("id")}/${t.getString("album_id")}" },
        )
        assertFalse("asked again, not given an old time", device.downloaded.any { it.second.has("added") })
        assertEquals(listOf("$a album:cratarr/al-1 One"), device.keptAgain)
        assertEquals("Ben's waits for his own sign-in", setOf(b), waiting())
    }

    @Test
    fun another_account_has_its_own_at_its_own_sign_in_and_nothing_of_anyone_else() {
        look()
        device.downloaded.clear()
        device.keptAgain.clear()
        device.who = b
        look()
        assertEquals(listOf("tr-2"), device.downloaded.map { it.second.getString("id") })
        assertTrue(device.downloaded.all { it.first == b })
        assertEquals(listOf("$b playlist:cratarr/pl-7 Mix"), device.keptAgain)
        assertEquals(emptySet<String>(), waiting())
        device.downloaded.clear()
        look()
        assertTrue("nothing twice", device.downloaded.isEmpty())
    }

    @Test
    fun the_first_to_sign_in_has_its_own_and_never_anothers() {
        device.who = b
        look()
        assertEquals(listOf("tr-2"), device.downloaded.map { it.second.getString("id") })
        assertEquals(listOf("$b playlist:cratarr/pl-7 Mix"), device.keptAgain)
        assertEquals(setOf(a), waiting())
    }

    @Test
    fun a_stop_while_clearing_clears_again_and_never_writes_down_a_half_cleared_device() {
        device.clearsLeft = 0
        look()
        assertEquals(null, device.marked)
        assertEquals(setOf(a, b), waiting())
        device.stored.remove("cratarr/tr-old1")
        device.clearsLeft = Int.MAX_VALUE
        look()
        assertEquals(now, device.marked)
        assertEquals("what was written down before the stop stands", listOf("tr-1", "tr-2"), device.downloaded.map { it.second.getString("id") })
    }

    @Test
    fun a_stop_before_the_mark_only_marks() {
        look(version = now)
        val first = device.snapshots
        device.marked = null
        device.who = "@nobody"
        look()
        assertEquals(now, device.marked)
        assertEquals("written down once", setOf(b), waiting())
        assertTrue(device.snapshots > first)
    }

    @Test
    fun once_done_never_again_and_an_app_from_before_does_nothing() {
        look(version = 1503)
        assertFalse(file.exists())
        assertEquals(3, device.stored.size)
        look()
        device.stored["cratarr/tr-new"] = track("tr-new", "al-1")
        device.downloaded.clear()
        look(version = now + 1)
        assertEquals(1, device.stored.size)
        assertTrue(device.downloaded.isEmpty())
    }

    @Test
    fun kept_lists_that_cannot_be_read_hold_it_all_back() {
        device.readable = false
        look()
        assertFalse(file.exists())
        assertEquals(null, device.marked)
        assertEquals(3, device.stored.size)
    }

    @Test
    fun the_fetch_waits_for_cratarr_for_the_downloads_network_and_for_lists_that_read() {
        device.reached = false
        look()
        assertEquals(now, device.marked)
        assertTrue(device.downloaded.isEmpty())
        device.reached = true
        device.may = false
        look()
        assertTrue("Wi-Fi only stays Wi-Fi only", device.downloaded.isEmpty())
        device.may = true
        device.readable = false
        look()
        assertTrue(device.downloaded.isEmpty())
        device.readable = true
        look()
        assertEquals(2, device.downloaded.size)
    }

    @Test
    fun a_list_that_cannot_be_read_is_never_written_down_again() {
        look()
        device.downloaded.clear()
        file.writeText("{not json")
        device.who = b
        look()
        assertTrue("nothing taken from a half-cleared device", device.downloaded.isEmpty())
        assertEquals(now, device.marked)
    }
}
