package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Two accounts on one device (decision 0101, point 13): a file stored once with a holder per account, each seeing only what it holds. */
class KeptHoldsTest {
    private val ann = "@ann"
    private val bo = "@bo"
    private fun manual(who: String) = KeptHolds.held(who, Kept.MANUAL)
    private fun item(who: String, key: String = "album:cratarr/al-1") = KeptHolds.held(who, key)
    private fun holders(vararg rows: Pair<String, Set<String>>) = rows.associate { (k, h) -> k to h.toMutableSet() }.toMutableMap()

    @Test
    fun an_account_sees_only_what_it_holds() {
        val h = holders("a" to setOf(manual(ann)), "b" to setOf(manual(bo)), "both" to setOf(manual(ann), manual(bo)))
        assertEquals(setOf("a", "both"), KeptHolds.seenBy(h, ann).keys)
        assertEquals(setOf("b", "both"), KeptHolds.seenBy(h, bo).keys)
    }

    @Test
    fun nobody_sees_none_of_an_accounts_and_an_account_none_of_nobodys() {
        val h = holders("hers" to setOf(manual(ann)), "the_devices" to setOf(manual(Who.NONE)))
        assertEquals(setOf("the_devices"), KeptHolds.seenBy(h, Who.NONE).keys)
        assertEquals(setOf("hers"), KeptHolds.seenBy(h, ann).keys)
    }

    @Test
    fun a_download_once_is_held_by_the_accounts_own_download_alone_whoever_else_holds_it() {
        val h = holders("alone" to setOf(manual(ann)), "shared" to setOf(manual(ann), manual(bo)), "synced" to setOf(manual(ann), item(ann)), "only_synced" to setOf(item(ann)))
        val seen = KeptHolds.seenBy(h, ann)
        assertEquals(true, seen["alone"])
        assertEquals("another's hold is nothing to hers", true, seen["shared"])
        assertEquals("under Synced only (the owner, 2026-10-24)", false, seen["synced"])
        assertEquals(false, seen["only_synced"])
    }

    @Test
    fun a_file_two_hold_stays_when_one_lets_go_and_goes_with_the_last() {
        val h = holders("both" to setOf(manual(ann), manual(bo)))
        assertEquals(emptyList<String>(), KeptHolds.letGo(h, listOf("both")) { it == manual(ann) })
        assertEquals(setOf(manual(bo)), h["both"])
        assertEquals(listOf("both"), KeptHolds.letGo(h, listOf("both")) { it == manual(bo) })
        assertFalse(h.containsKey("both"))
    }

    @Test
    fun an_accounts_own_kept_item_keeps_a_file_its_download_let_go_of() {
        val h = holders("a" to setOf(manual(ann), item(ann)))
        assertEquals(emptyList<String>(), KeptHolds.letGo(h, listOf("a")) { it == manual(ann) })
        assertEquals(setOf(item(ann)), h["a"])
    }

    @Test
    fun an_accounts_remove_takes_all_its_holds_and_no_others() {
        val h = holders("hers" to setOf(manual(ann), item(ann)), "both" to setOf(item(ann), manual(bo)), "his" to setOf(manual(bo)))
        val gone = KeptHolds.letGo(h, listOf("hers", "both")) { KeptHolds.whoOf(it) == ann }
        assertEquals(listOf("hers"), gone)
        assertEquals(setOf(manual(bo)), h["both"])
        assertEquals(setOf(manual(bo)), h["his"])
    }

    @Test
    fun a_remove_goes_no_further_than_what_it_was_asked_about() {
        // A download with no holders yet is one just added, its hold on its way: a remove of something else leaves it.
        val h = holders("a" to setOf(manual(ann)))
        assertEquals(listOf("a"), KeptHolds.letGo(h, listOf("a", "just_added")) { it == manual(ann) })
        assertTrue(h.isEmpty())
    }

    @Test
    fun a_pass_gives_a_download_no_holder_it_did_not_have() {
        // One told to go is in the store a while yet: given to whoever is signed in, its hold would outlast it, and show
        // that one the next account's copy of it.
        val h = holders("hers" to setOf(manual(ann)))
        assertEquals(emptyList<String>(), KeptHolds.afterPass(h, emptyMap()))
        assertEquals(setOf("hers"), h.keys)
    }

    @Test
    fun a_pass_keeps_every_accounts_own_downloads_and_the_items_holds_the_budget_gave() {
        val h = holders("hers" to setOf(manual(ann)), "his" to setOf(manual(bo)), "left_her_item" to setOf(item(ann)), "still_in_it" to setOf(item(ann)))
        val gone = KeptHolds.afterPass(h, mapOf("still_in_it" to setOf(item(ann)), "new" to setOf(item(bo))))
        assertEquals(listOf("left_her_item"), gone)
        assertEquals(setOf("hers", "his", "still_in_it", "new"), h.keys)
    }

    @Test
    fun an_items_old_hold_goes_with_a_pass_and_an_accounts_own_download_does_not() {
        val h = holders("a" to setOf(manual(ann), item(ann)))
        assertEquals(emptyList<String>(), KeptHolds.afterPass(h, emptyMap()))
        assertEquals(setOf(manual(ann)), h["a"])
    }

    @Test
    fun each_accounts_share_counts_a_file_it_holds_once_and_one_two_hold_in_both() {
        val h = holders("hers" to setOf(manual(ann), item(ann)), "both" to setOf(manual(ann), manual(bo)), "gone_from_the_store" to setOf(manual(bo)))
        val bytes = mapOf("hers" to 10L, "both" to 5L)
        assertEquals(mapOf(ann to 15L, bo to 5L), KeptHolds.shares(h) { bytes[it] })
    }

    // What runs only on a device, read from its source: says a line is there, not that it works.

    private val kept = File("src/main/java/app/cratarr/Kept.kt").readText().replace("\r\n", "\n")

    @Test
    fun whose_a_keep_or_a_hold_or_a_remove_is_goes_by_who_asked_not_by_who_is_signed_in_when_the_worker_comes_to_it() {
        for (f in listOf("fun set(", "fun holdManual(", "fun releaseManual(")) {
            val asked = kept.substringAfter(f).substringBefore("worker.execute")
            assertTrue(f, "Who.current(app)" in asked)
            val work = kept.substringAfter(f).substringAfter("worker.execute").substringBefore("\n    }\n")
            assertFalse(f, "Who.current" in work)
        }
    }

    @Test
    fun a_remove_looks_only_at_what_it_was_asked_about_never_at_the_whole_store() {
        val release = kept.substringAfter("private fun release(").substringBefore("\n    }\n")
        assertTrue("KeptHolds.letGo(holders, keys, gone)" in release)
        assertFalse("downloadIndex" in release)
        assertFalse("downloadedKeys" in release)
    }

    @Test
    fun a_pass_gives_nothing_to_the_one_signed_in_for_having_no_holder() {
        val pass = kept.substringAfter("private fun syncHere(").substringBefore("private fun librariesOff(")
        assertFalse("KeptHolds.held(" in pass)
        assertFalse("isNullOrEmpty" in pass)
    }

    @Test
    fun what_shows_of_the_holders_is_none_while_they_cannot_be_read_and_what_changes_them_stops() {
        val load = kept.substringAfter("private fun loadHolders(").substringBefore("\n")
        assertTrue(load, "KeptFile.holders(" in load)
        for (f in listOf("fun heldHere(", "fun report(")) {
            val shows = kept.substringAfter(f).substringBefore("\n    }\n")
            assertTrue(f, "shownHolders(app)" in shows)
            assertFalse(f, "loadHolders(" in shows)
        }
    }

}
