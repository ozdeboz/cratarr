package app.cratarr

import app.cratarr.rules.SyncKind
import app.cratarr.rules.SyncMobile
import app.cratarr.rules.SyncNetwork
import app.cratarr.rules.SyncOutcome
import app.cratarr.rules.SyncPart
import app.cratarr.rules.SyncStep
import app.cratarr.rules.SyncTaken
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * The app's sync of the library (IndexLooks, IndexFiles), on real files, with stand-ins for Cratarr and for the rule. The rule is the
 * crate's, proved there and by the bridge's own tests, and these have no library: all that is held here is what Kotlin does with its
 * answers, what it asks for, what it keeps, and whose that is.
 */
class DeviceIndexTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val whole = "whole".toByteArray()
    private val sounds = "sounds".toByteArray()
    private val feed = "+feed".toByteArray()
    private val otherVersion = "other version".toByteArray()
    private val badFeed = "bad feed".toByteArray()

    /** Small covers as the server writes them. */
    private fun head(rev: Long, version: Int = 1): ByteArray =
        ByteBuffer.allocate(16).order(ByteOrder.LITTLE_ENDIAN).put("CRXV".toByteArray()).putInt(version).putLong(rev).array()

    private fun frame(record: Long, jpeg: ByteArray = ByteArray(0)): ByteArray =
        ByteBuffer.allocate(12 + jpeg.size).order(ByteOrder.LITTLE_ENDIAN).putLong(record).putInt(jpeg.size).put(jpeg).array()

    private fun picture(n: Int) = ByteArray(40) { n.toByte() }

    private fun bundle(rev: Long, vararg frames: ByteArray) = frames.fold(head(rev)) { all, f -> all + f }

    private inner class StandIn : IndexRule {
        var says: ((SyncPart) -> SyncStep)? = null
        val starts = mutableListOf<String>()
        var network: SyncNetwork? = null
        var mobile: SyncMobile? = null

        override fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, told: Boolean, now: Long): SyncStep {
            this.network = network
            this.mobile = mobile
            return says?.invoke(part) ?: when {
                part.stopped -> SyncStep.Stopped
                part.lost > 0u || part.failed > 0u -> SyncStep.Wait(now + 1)
                part.whole -> SyncStep.Whole
                told -> SyncStep.Changes
                else -> SyncStep.Rest
            }
        }

        /** What the rule says of an account's own, by when it was last taken. */
        var stale: (Long?) -> SyncStep = { SyncStep.Rest }

        override fun stepStale(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, took: Long?, now: Long): SyncStep = stale(took)

        override fun after(part: SyncPart, outcome: SyncOutcome, now: Long) = when (outcome) {
            SyncOutcome.TOOK -> part.copy(whole = false, lost = 0u, failed = 0u, tried = now)
            SyncOutcome.LOST -> part.copy(lost = part.lost + 1u, tried = now)
            SyncOutcome.FAILED -> part.copy(failed = part.failed + 1u, lost = 0u, tried = now)
            SyncOutcome.START_AGAIN -> part.copy(whole = true, lost = 0u, tried = now)
            SyncOutcome.VERSION -> part.copy(stopped = true, whole = true, lost = 0u, tried = now)
        }

        override fun anew(part: SyncPart, now: Long): SyncPart {
            starts.add("plain")
            return part.copy(lost = 0u, failed = 0u, tried = null, started = now)
        }

        override fun anewByAnother(part: SyncPart, now: Long): SyncPart {
            starts.add("by another")
            return part.copy(whole = true, stopped = false, lost = 0u, failed = 0u, tried = null, started = now)
        }

        override fun paused(part: SyncPart) = part.lost >= 3u

        override fun answer(status: Int?, whole: Boolean): SyncOutcome? = when (status) {
            null, 502 -> SyncOutcome.LOST
            200 -> null
            204 -> if (whole) SyncOutcome.FAILED else SyncOutcome.TOOK
            410 -> if (whole) SyncOutcome.FAILED else SyncOutcome.START_AGAIN
            else -> SyncOutcome.FAILED
        }

        override fun version(kind: SyncKind) = 3

        /** What the head of covers says: `null` where it is not whole. */
        private fun headRev(head: ByteArray) = head.takeIf { it.size == 16 }?.let { ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).getLong(8) }

        private fun headVersion(head: ByteArray) = ByteBuffer.wrap(head).order(ByteOrder.LITTLE_ENDIAN).getInt(4)

        override fun takeWhole(kind: SyncKind, bytes: ByteArray) = when {
            kind == SyncKind.COVERS -> when {
                headRev(bytes) == null -> SyncTaken(SyncOutcome.FAILED, null, null)
                headVersion(bytes) != 1 -> SyncTaken(SyncOutcome.VERSION, null, null)
                else -> SyncTaken(SyncOutcome.TOOK, headRev(bytes), null)
            }
            bytes.contentEquals(otherVersion) -> SyncTaken(SyncOutcome.VERSION, null, null)
            else -> SyncTaken(SyncOutcome.TOOK, 5L, null)
        }

        override fun takeChanges(kind: SyncKind, kept: ByteArray, feed: ByteArray) = when {
            kind == SyncKind.COVERS ->
                if (headRev(feed) == null || headVersion(feed) != 1) SyncTaken(SyncOutcome.START_AGAIN, null, null) else SyncTaken(SyncOutcome.TOOK, headRev(feed), null)
            feed.contentEquals(badFeed) -> SyncTaken(SyncOutcome.START_AGAIN, null, null)
            else -> SyncTaken(SyncOutcome.TOOK, 9L, kept + feed)
        }
    }

    private val rule = StandIn()
    private val asked = mutableListOf<String>()
    /** What the log was asked, apart from the rest, and what it says of the rows. */
    private val askedLog = mutableListOf<String>()
    private var moved = true
    /** How Cratarr answers each address, by its path without `?`. */
    private val answers = mutableMapOf<String, () -> IndexFetched>()
    private var who = "@ann"
    private var clock = 1_000L
    private lateinit var files: IndexFiles
    private lateinit var looks: IndexLooks

    @Before
    fun made() {
        files = IndexFiles(folder.root)
        looks = IndexLooks(rule, files, ::opened, { who }, { SyncNetwork.METERED }, { SyncMobile.YES }, { clock })
    }

    private fun opened(path: String, read: (Int?, InputStream?) -> Unit) {
        val got = fetch(path)
        read(got.status, broken ?: got.bytes?.let { ByteArrayInputStream(it) })
    }

    /** A body that goes as it is read, for one test. */
    private var broken: InputStream? = null

    private fun fetch(path: String): IndexFetched {
        val p = path.substringBefore('?')
        if (p == "/api/changes") {
            askedLog.add(path)
            return answers[p]?.invoke() ?: IndexFetched(200, """{"rev":1,"changes":[],"more":false,"index":{"rev":9,"changed":$moved}}""".toByteArray())
        }
        asked.add(path)
        answers[p]?.let { return it() }
        return when (p) {
            "/api/index" -> IndexFetched(200, whole)
            "/api/index/sounds" -> IndexFetched(200, sounds)
            "/api/index/covers" -> IndexFetched(200, bundle(5, frame(1, picture(1)), frame(2, picture(2))))
            "/api/index/own" -> IndexFetched(200, whole)
            else -> IndexFetched(204, null)
        }
    }

    private val wholes = listOf("/api/index", "/api/index/sounds", "/api/index/covers")
    private val neverTaken = { took: Long? -> if (took == null) SyncStep.Whole else SyncStep.Rest }
    private val coverFeed = "/api/index/covers/changes?since=5"
    private fun coverFiles() = File(files.bytes(who, "index").parentFile, "covers").list()?.sorted() ?: emptyList()

    private fun bytes(name: String, of: String = who) = files.bytes(of, name).readBytes()

    /** Every part held, and a new start over them: as an app opened again leaves it. */
    private fun held() {
        rule.stale = neverTaken
        looks.look(who)
        asked.clear()
        clock += 1_000
        looks.startAnew()
    }

    @Test
    fun whole_keeps_the_bytes_as_fetched_each_part_at_its_rev_and_dated() {
        clock = 2_000
        looks.look(who)
        assertEquals(wholes, asked)
        assertArrayEquals(whole, bytes("index"))
        assertArrayEquals(sounds, bytes("sounds"))
        val stands = files.stands(who, "index")!!
        assertEquals(5L, stands.rev)
        assertEquals(2_000L, stands.at)
        assertFalse(stands.part.whole)
    }

    @Test
    fun an_accounts_own_goes_whole_by_the_rule_for_what_has_no_feed_and_is_kept_as_it_came() {
        rule.stale = neverTaken
        clock = 2_000
        looks.look(who)
        assertEquals(wholes + "/api/index/own", asked)
        assertArrayEquals(whole, bytes("own"))
        assertEquals(2_000L, files.stands(who, "own")?.at)
        asked.clear()
        looks.look(who, told = true)
        assertFalse("told or not, it goes by when it was taken", asked.any { it.startsWith("/api/index/own") })
    }

    @Test
    fun an_accounts_own_says_nothing_of_how_the_library_stands() {
        rule.stale = { SyncStep.Ask }
        val steps = looks.look(who)
        assertEquals(SyncStep.Ask, steps["own"])
        assertEquals("the row speaks of the library", listOf(false, false, false), looks.facts(steps).map { it.asks })
    }

    @Test
    fun what_is_kept_says_the_version_that_kept_it_whatever_came() {
        answers["/api/index"] = { IndexFetched(null, null) }
        looks.look(who)
        assertEquals(3, files.stands(who, "index")?.wants)
        assertEquals(3, files.stands(who, "sounds")?.wants)
    }

    @Test
    fun with_bytes_held_a_look_asks_for_what_changed_since_where_each_part_stands() {
        held()
        looks.look(who)
        assertEquals(listOf("/api/index/changes?since=5", "/api/index/sounds/changes?since=5", coverFeed), asked)
    }

    @Test
    fun a_look_asks_the_log_and_no_feed_of_rows_or_sounds_when_nothing_moved_as_each_is_a_whole_build() {
        held()
        moved = false
        looks.look(who)
        assertEquals(listOf("/api/changes?since=0&kinds=index&index=5"), askedLog)
        assertEquals(listOf(coverFeed), asked)
    }

    @Test
    fun nothing_new_of_covers_moves_their_date_and_counts_as_no_failure() {
        held()
        clock = 9_000
        moved = false
        looks.look(who)
        assertEquals(9_000L, files.stands(who, "covers")?.at)
        assertEquals(0u, files.stands(who, "covers")?.part?.failed)
    }

    @Test
    fun told_by_the_page_a_look_goes_to_the_feeds_with_no_asking() {
        held()
        moved = false
        looks.look(who, told = true)
        assertEquals(emptyList<String>(), askedLog)
        assertEquals(listOf("/api/index/changes?since=5", "/api/index/sounds/changes?since=5", coverFeed), asked)
    }

    @Test
    fun nothing_held_asks_the_log_nothing() {
        looks.look(who)
        assertEquals(emptyList<String>(), askedLog)
    }

    @Test
    fun a_log_that_answers_nothing_it_can_read_says_nothing_moved() {
        held()
        for (answer in listOf(IndexFetched(null, null), IndexFetched(200, "{ cut".toByteArray()), IndexFetched(200, "{}".toByteArray()))) {
            answers["/api/changes"] = { answer }
            asked.clear()
            looks.look(who)
            assertEquals(listOf(coverFeed), asked)
        }
    }

    @Test
    fun each_feed_goes_from_its_own_rev() {
        held()
        answers["/api/index/changes"] = { IndexFetched(200, feed) }
        answers["/api/index/covers/changes"] = { IndexFetched(200, bundle(12)) }
        looks.look(who)
        asked.clear()
        answers.clear()
        looks.look(who)
        assertEquals(listOf("/api/index/changes?since=9", "/api/index/sounds/changes?since=5", "/api/index/covers/changes?since=12"), asked)
    }

    @Test
    fun whole_covers_lie_a_file_each_at_the_rev_in_their_own_head() {
        looks.look(who)
        assertEquals(listOf("1.jpg", "2.jpg"), coverFiles())
        assertArrayEquals(picture(2), files.cover(who, 2).readBytes())
        assertEquals(5L, files.stands(who, "covers")?.rev)
    }

    @Test
    fun a_feed_of_covers_brings_what_was_made_and_drops_what_has_none_now() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(200, bundle(7, frame(3, picture(3)), frame(1), frame(2, picture(9)))) }
        looks.look(who)
        assertEquals(listOf("2.jpg", "3.jpg"), coverFiles())
        assertArrayEquals(picture(9), files.cover(who, 2).readBytes())
        assertEquals(7L, files.stands(who, "covers")?.rev)
        assertEquals("a cover moves no other part", 5L, files.stands(who, "index")?.rev)
    }

    @Test
    fun start_again_on_covers_fetches_them_whole_goes_on_from_the_rev_inside_and_what_is_gone_goes() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(410, "{\"rev\":99,\"reset\":true}".toByteArray()) }
        answers["/api/index/covers"] = { IndexFetched(200, bundle(20, frame(2, picture(2)))) }
        looks.look(who)
        assertEquals(listOf("2.jpg"), coverFiles())
        assertEquals(20L, files.stands(who, "covers")?.rev)
    }

    @Test
    fun a_whole_cut_short_keeps_the_covers_held_before_it_until_the_new_ones_are_all_in() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(410, "{\"rev\":99,\"reset\":true}".toByteArray()) }
        val fresh = bundle(20, frame(2, picture(9)), frame(3, picture(3)))
        answers["/api/index/covers"] = { IndexFetched(200, fresh.copyOf(fresh.size - 5)) }
        looks.look(who)
        assertEquals("what came of it taken, nothing gone for what did not", listOf("1.jpg", "2.jpg"), coverFiles())
        assertArrayEquals(picture(9), files.cover(who, 2).readBytes())
        assertEquals(5L, files.stands(who, "covers")?.rev)
        answers["/api/index/covers"] = { IndexFetched(200, fresh) }
        clock += 1_000
        looks.startAnew()
        looks.look(who)
        assertEquals("whole, what it no longer holds goes", listOf("2.jpg", "3.jpg"), coverFiles())
        assertEquals(20L, files.stands(who, "covers")?.rev)
    }

    @Test
    fun covers_of_another_version_in_a_whole_stop_and_what_was_held_stays() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(410, null) }
        answers["/api/index/covers"] = { IndexFetched(200, head(20, version = 9) + frame(9, picture(9))) }
        looks.look(who)
        assertTrue(files.stands(who, "covers")!!.part.stopped)
        assertEquals(listOf("1.jpg", "2.jpg"), coverFiles())
    }

    @Test
    fun a_feed_of_covers_of_another_version_asks_for_the_whole() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(200, head(20, version = 9)) }
        looks.look(who)
        assertTrue(asked.contains("/api/index/covers"))
        assertFalse(files.stands(who, "covers")!!.part.stopped)
    }

    @Test
    fun covers_cut_short_count_as_the_rule_says_and_their_rev_stays() {
        held()
        val cut = bundle(7, frame(3, picture(3)))
        answers["/api/index/covers/changes"] = { IndexFetched(200, cut.copyOf(cut.size - 5)) }
        answers["/api/index/covers"] = { IndexFetched(200, cut.copyOf(cut.size - 5)) }
        looks.look(who)
        val stands = files.stands(who, "covers")!!
        assertEquals(5L, stands.rev)
        assertTrue("the feed started again", stands.part.whole)
        assertEquals("and the whole after it failed", 1u, stands.part.failed)
    }

    @Test
    fun a_frame_cut_in_its_own_head_is_cut_short_too() {
        answers["/api/index/covers"] = { IndexFetched(200, head(7) + ByteArray(5)) }
        looks.look(who)
        assertEquals(1u, files.stands(who, "covers")?.part?.failed)
    }

    @Test
    fun a_body_of_nothing_is_no_covers() {
        answers["/api/index/covers"] = { IndexFetched(200, ByteArray(0)) }
        looks.look(who)
        assertEquals(1u, files.stands(who, "covers")?.part?.failed)
        assertNull(files.stands(who, "covers")?.rev)
    }

    @Test
    fun a_connection_going_during_covers_is_lost_and_counts_for_nothing() {
        held()
        answers["/api/index/covers/changes"] = { IndexFetched(200, ByteArray(0)) }
        broken = object : InputStream() {
            override fun read(): Int = throw IOException("gone")
        }
        looks.look(who)
        broken = null
        assertEquals(1u, files.stands(who, "covers")?.part?.lost)
        assertEquals(0u, files.stands(who, "covers")?.part?.failed)
    }

    @Test
    fun a_feed_taken_keeps_what_the_rule_gave_back_at_its_rev_and_moves_the_date() {
        held()
        clock = 9_000
        answers["/api/index/changes"] = { IndexFetched(200, feed) }
        looks.look(who)
        assertArrayEquals(whole + feed, bytes("index"))
        assertEquals(9L, files.stands(who, "index")?.rev)
        assertEquals(9_000L, files.stands(who, "index")?.at)
    }

    @Test
    fun nothing_changed_moves_the_date_and_leaves_the_bytes_and_where_they_stand() {
        held()
        clock = 9_000
        looks.look(who)
        assertArrayEquals(whole, bytes("index"))
        assertEquals(5L, files.stands(who, "index")?.rev)
        assertEquals(9_000L, files.stands(who, "index")?.at)
    }

    @Test
    fun start_again_fetches_it_whole_in_the_same_look() {
        held()
        answers["/api/index/changes"] = { IndexFetched(410, null) }
        looks.look(who)
        assertEquals(listOf("/api/index/changes?since=5", "/api/index", "/api/index/sounds/changes?since=5", coverFeed), asked)
        assertFalse(files.stands(who, "index")!!.part.whole)
    }

    @Test
    fun a_feed_the_rule_will_not_take_fetches_it_whole_and_leaves_the_bytes_till_then() {
        held()
        answers["/api/index/changes"] = { IndexFetched(200, badFeed) }
        answers["/api/index"] = { IndexFetched(null, null) }
        looks.look(who)
        assertArrayEquals("what is held stays readable", whole, bytes("index"))
        assertTrue(files.stands(who, "index")!!.part.whole)
    }

    @Test
    fun held_with_no_file_of_bytes_fetches_it_whole() {
        held()
        files.bytes(who, "index").delete()
        looks.look(who)
        assertEquals("/api/index", asked.first())
        assertArrayEquals(whole, bytes("index"))
    }

    @Test
    fun a_lost_fetch_keeps_the_bytes_and_what_the_rule_counted() {
        held()
        val at = files.stands(who, "index")?.at
        clock += 1_000
        answers["/api/index/changes"] = { IndexFetched(null, null) }
        looks.look(who)
        assertArrayEquals(whole, bytes("index"))
        assertEquals(1u, files.stands(who, "index")?.part?.lost)
        assertEquals(0u, files.stands(who, "index")?.part?.failed)
        assertEquals("it stood where Cratarr does no later than it did", at, files.stands(who, "index")?.at)
    }

    @Test
    fun an_answer_that_should_have_brought_bytes_and_did_not_is_a_lost_connection() {
        answers["/api/index"] = { IndexFetched(200, null) }
        looks.look(who)
        assertEquals(1u, files.stands(who, "index")?.part?.lost)
        assertFalse(files.bytes(who, "index").exists())
    }

    @Test
    fun the_version_the_rule_stops_at_keeps_what_was_there_and_says_so() {
        held()
        answers["/api/index/changes"] = { IndexFetched(410, null) }
        answers["/api/index"] = { IndexFetched(200, otherVersion) }
        looks.look(who)
        assertArrayEquals(whole, bytes("index"))
        val stands = files.stands(who, "index")!!
        assertTrue(stands.part.stopped)
        assertEquals(3, stands.wants)
        assertEquals(5L, stands.rev)
    }

    @Test
    fun a_start_over_what_this_version_kept_is_a_plain_start() {
        held()
        rule.starts.clear()
        looks.look(who)
        assertEquals(listOf("plain", "plain", "plain", "plain"), rule.starts)
    }

    @Test
    fun a_start_over_what_another_version_kept_is_told_as_that_and_the_rule_says_what_it_means() {
        held()
        rule.starts.clear()
        val stands = files.stands(who, "index")!!
        files.keep(who, "index", IndexStands(stands.rev, stands.at, 2, stands.part), null)
        looks.look(who)
        assertEquals(listOf("by another", "plain", "plain", "plain"), rule.starts)
        assertEquals(listOf("/api/index", "/api/index/sounds/changes?since=5", coverFeed), asked)
    }

    @Test
    fun a_part_is_taken_from_what_is_kept_once_a_start() {
        held()
        rule.starts.clear()
        looks.look(who)
        looks.look(who)
        assertEquals(4, rule.starts.size)
    }

    @Test
    fun a_start_takes_each_part_from_what_is_kept_again() {
        held()
        looks.look(who)
        rule.starts.clear()
        looks.startAnew()
        looks.look(who)
        assertEquals(4, rule.starts.size)
    }

    @Test
    fun another_account_coming_during_a_fetch_keeps_nothing_of_it() {
        answers["/api/index"] = {
            who = "@bob"
            IndexFetched(200, whole)
        }
        val steps = looks.look("@ann")
        assertEquals(SyncStep.Rest, steps["index"])
        assertFalse(files.bytes("@ann", "index").exists())
        assertFalse(files.bytes("@bob", "index").exists())
        assertNull(files.stands("@ann", "index"))
    }

    @Test
    fun a_look_for_one_who_left_fetches_nothing() {
        who = "@bob"
        assertEquals(SyncStep.Rest, looks.look("@ann")["index"])
        assertEquals(emptyList<String>(), asked)
    }

    @Test
    fun each_accounts_files_lie_apart() {
        looks.look("@ann")
        who = "@bob"
        looks.startAnew()
        answers["/api/index"] = { IndexFetched(200, "bob's".toByteArray()) }
        looks.look("@bob")
        assertArrayEquals(whole, bytes("index", "@ann"))
        assertArrayEquals("bob's".toByteArray(), bytes("index", "@bob"))
    }

    @Test
    fun a_name_with_a_slash_stays_in_a_folder_of_its_own() {
        val odd = "@../ann"
        who = odd
        looks.look(odd)
        assertEquals(folder.root.canonicalPath, files.bytes(odd, "index").parentFile!!.parentFile!!.canonicalPath)
    }

    @Test
    fun storage_refusing_what_was_fetched_is_a_failure_waited_on_with_no_date() {
        // A file where the account's folder would go: nothing can be written there.
        File(folder.root, java.net.URLEncoder.encode(who, "UTF-8")).writeText("in the way")
        val steps = looks.look(who)
        assertEquals(wholes, asked)
        assertTrue("it waits as any failure does", steps["index"] is SyncStep.Wait)
        assertNull(files.stands(who, "index"))
    }

    @Test
    fun a_rule_that_keeps_asking_ends_with_the_look() {
        rule.says = { SyncStep.Whole }
        answers["/api/index"] = { IndexFetched(500, null) }
        answers["/api/index/sounds"] = { IndexFetched(500, null) }
        answers["/api/index/covers"] = { IndexFetched(500, null) }
        looks.look(who)
        assertEquals(9, asked.size)
    }

    @Test
    fun the_rule_goes_by_the_network_and_the_answer_it_is_handed() {
        looks.look(who)
        assertEquals(SyncNetwork.METERED, rule.network)
        assertEquals(SyncMobile.YES, rule.mobile)
    }

    @Test
    fun what_the_rule_says_to_ask_or_to_wait_fetches_nothing_and_is_passed_on() {
        rule.says = { SyncStep.Ask }
        val steps = looks.look(who)
        assertEquals(SyncStep.Ask, steps["index"])
        assertEquals(emptyList<String>(), asked)
        val facts = looks.facts(steps)
        assertEquals(listOf(true, true, true), facts.map { it.asks })
        assertEquals(listOf(true, true, true), facts.map { it.whole })
    }

    @Test
    fun the_page_is_told_each_part_as_the_look_left_it() {
        answers["/api/index"] = { IndexFetched(200, otherVersion) }
        val facts = looks.facts(looks.look(who))
        assertEquals(listOf(true, false, false), facts.map { it.stopped })
        assertEquals(listOf(false, false, false), facts.map { it.asks })
    }

    @Test
    fun paused_is_the_rules_to_say() {
        held()
        answers["/api/index/changes"] = { IndexFetched(null, null) }
        rule.says = { if (it.lost < 3u) SyncStep.Changes else SyncStep.Wait(0) }
        val facts = looks.facts(looks.look(who))
        assertEquals(listOf(true, false, false), facts.map { it.paused })
    }

    @Test
    fun a_tap_lifts_what_stopped_in_this_start() {
        answers["/api/index"] = { IndexFetched(200, otherVersion) }
        looks.look(who)
        answers.remove("/api/index")
        looks.look(who)
        assertFalse("still stopped until then", files.bytes(who, "index").exists())
        looks.tryAgain()
        looks.look(who)
        assertArrayEquals(whole, bytes("index"))
        assertFalse(files.stands(who, "index")!!.part.stopped)
    }

    @Test
    fun a_write_cut_off_leaves_what_was_kept_whole() {
        looks.look(who)
        val stands = files.stands(who, "index")!!
        assertThrows(IOException::class.java) {
            files.keep(who, "index", stands, "longer than it was".toByteArray()) { to, bytes ->
                to.writeBytes(bytes.copyOf(3))
                throw IOException("no room left")
            }
        }
        assertArrayEquals(whole, bytes("index"))
    }

    @Test
    fun a_stop_between_the_two_files_leaves_the_bytes_ahead_of_their_word() {
        looks.look(who)
        val stands = files.stands(who, "index")!!
        var puts = 0
        assertThrows(IOException::class.java) {
            files.keep(who, "index", IndexStands(9L, 8L, 3, stands.part), "new".toByteArray()) { to, bytes ->
                if (puts++ == 1) throw IOException("stopped")
                to.writeBytes(bytes)
            }
        }
        assertArrayEquals("new".toByteArray(), bytes("index"))
        assertEquals("a feed from here is refused, and a whole mends it", 5L, files.stands(who, "index")?.rev)
    }

    @Test
    fun what_stands_unreadable_reads_as_nothing_kept_and_a_whole_mends_it() {
        looks.look(who)
        File(files.bytes(who, "index").parentFile, "index.json").writeText("{ cut")
        assertNull(files.stands(who, "index"))
        looks.startAnew()
        asked.clear()
        looks.look(who)
        assertEquals("/api/index", asked.first())
        assertEquals(5L, files.stands(who, "index")?.rev)
    }

    @Test
    fun how_a_part_stands_reads_back_as_it_was_kept() {
        val part = SyncPart(whole = true, stopped = true, lost = 2u, failed = 1u, tried = 7L, started = 99L)
        files.keep(who, "index", IndexStands(4L, 6L, 3, part), null)
        val read = files.stands(who, "index")!!
        assertEquals(4L, read.rev)
        assertEquals(6L, read.at)
        assertEquals(3, read.wants)
        assertEquals("a start's own time is not kept", part.copy(started = 0L), read.part)
    }

    @Test
    fun a_small_covers_address_names_its_record_and_no_other_address_does() {
        assertEquals(10L, coverRecord("/__device/index/cover/10"))
        assertEquals(null, coverRecord("/__device/index"))
        assertEquals(null, coverRecord("/__device/cover/10"))
        assertEquals(null, coverRecord("/__device/index/cover/"))
        assertEquals(null, coverRecord("/__device/index/cover/al-10"))
    }

    /** The worker's fetch reaches the app by its own client, not the page's: asked of one alone, a list away from Cratarr shows no cover. */
    @Test
    fun the_service_worker_asks_a_small_cover_where_the_app_answers_it() {
        assertTrue(File("../../web/public/sw.js").readText().contains("const INDEX_COVER = '${DeviceIndex.COVER_PATH}';"))
        val worker = source("MainActivity.kt").substringAfter("setServiceWorkerClient(").substringBefore("view.webChromeClient")
        assertTrue("the small cover first", worker.contains("return request.url.path?.let { DeviceIndex.answer(app, it) }"))
        assertTrue("then the page's files", worker.contains("?: BundledPage.answer(app, request.method, request.url)"))
    }

    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")
    private fun page(name: String) = File("../../web/src/lib/$name").readText().replace("\r\n", "\n")

    @Test
    fun the_app_opening_starts_it_and_so_does_its_service_opened_alone() {
        assertTrue(source("MainActivity.kt").contains("DeviceIndex.start(this)"))
        assertTrue("a car opens this with no page", source("PlayerService.kt").contains("DeviceIndex.start(this)"))
    }

    @Test
    fun another_accounts_things_on_the_device_are_a_start() {
        assertTrue(source("Who.kt").contains("DeviceIndex.start(app)"))
    }

    @Test
    fun a_new_network_looks_once_the_address_is_settled() {
        // 0.16's fresh start looks then too (decision 0101 point 23, FreshStart): what waits may be fetched on the new network.
        assertTrue(source("Reach.kt").contains("checkSoon(app, SETTLE_MS) { DeviceIndex.look(app); FreshStart.look(app) }"))
    }

    @Test
    fun the_worker_runs_on_any_connection_as_changes_go_on_any() {
        val kept = source("DeviceIndex.kt")
        assertTrue(kept.contains("setRequiredNetworkType(NetworkType.CONNECTED)"))
        assertFalse(kept.contains("UNMETERED"))
        assertTrue(source("MainActivity.kt").contains("DeviceIndex.schedule(this)"))
    }

    @Test
    fun a_wait_is_kept_only_with_the_page_in_view() {
        assertTrue(source("DeviceIndex.kt").contains("if (inView) steps.values"))
        val activity = source("MainActivity.kt")
        assertTrue(activity.contains("DeviceIndex.shown(this)"))
        assertTrue(activity.contains("DeviceIndex.hidden()"))
    }

    @Test
    fun the_page_hears_how_it_stands_when_it_opens_and_after_each_look() {
        val activity = source("MainActivity.kt")
        assertTrue(activity.contains("reply.postMessage(DeviceIndex.report(this).toString())"))
        assertTrue(activity.contains("DeviceIndex.onChange = { runOnUiThread { page?.postMessage(DeviceIndex.report(this).toString()) } }"))
        assertTrue("let go with the activity", activity.contains("DeviceIndex.onChange = null"))
    }

    @Test
    fun what_the_page_sends_is_what_the_app_listens_for() {
        val activity = source("MainActivity.kt")
        val sync = page("deviceSync.svelte.ts")
        for (type in listOf("index-look", "index-again")) {
            assertTrue(type, sync.contains("tellApp('$type'"))
            assertTrue(type, activity.contains("\"$type\" -> "))
        }
    }

    @Test
    fun the_page_says_whether_it_was_told_and_the_app_hears_it() {
        assertTrue(page("deviceSync.svelte.ts").contains("toApp({ type, told })"))
        assertTrue(source("MainActivity.kt").contains("DeviceIndex.look(this, message.optBoolean(\"told\"))"))
    }

    @Test
    fun what_the_app_tells_is_what_the_page_listens_for() {
        val sync = page("deviceSync.svelte.ts")
        val told = source("DeviceIndex.kt").substringAfter("fun report(").substringBefore("fun answer(")
        assertTrue(sync.contains("onApp('device-index',"))
        assertTrue(told.contains(".put(\"type\", \"device-index\")"))
        for (field in listOf("at", "rev", "covers", "metered", "parts")) assertTrue(field, told.contains(".put(\"$field\","))
        for (field in listOf("whole", "stopped", "paused", "asks")) assertTrue(field, told.contains(".put(\"$field\", it.$field)"))
    }

    @Test
    fun the_page_reads_the_bytes_at_the_address_the_app_answers_and_the_date_with_them() {
        val reads = page("deviceIndex.svelte.ts")
        // From the source: a constant is compiled into this test, and would agree with itself.
        val path = Regex("PAGE_PATH = \"(.*?)\"").find(source("DeviceIndex.kt"))!!.groupValues[1]
        assertTrue(reads.contains("'$path'"))
        assertTrue(reads.contains("X-Cratarr-At"))
        assertTrue(source("DeviceIndex.kt").contains("\"X-Cratarr-At\" to"))
        assertTrue(source("CratarrWebViewClient.kt").contains("url.path?.let(deviceIndex)?.let { return it }"))
        assertTrue(source("MainActivity.kt").contains("deviceIndex = { DeviceIndex.answer(app, it) }"))
    }

    @Test
    fun the_persons_answer_is_read_from_the_devices_settings_where_the_page_keeps_it() {
        val prefs = page("prefs.svelte.ts")
        assertTrue(source("DeviceIndex.kt").contains("Prefs(app).pagePrefs ?: \"{}\").optString(\"indexMobile\")"))
        assertTrue(prefs.contains("indexMobile: 'unasked'"))
        val personal = Regex("const PERSONAL = \\[(.*?)\\]", RegexOption.DOT_MATCHES_ALL).find(prefs)!!.groupValues[1]
        assertFalse("not the account's own, or the app would read it in another place", personal.contains("indexMobile"))
    }

    @Test
    fun nothing_known_reads_back_as_nothing() {
        val part = SyncPart(whole = true, stopped = false, lost = 0u, failed = 0u, tried = null, started = 0L)
        files.keep(who, "index", IndexStands(null, null, null, part), null)
        val read = files.stands(who, "index")!!
        assertNull(read.rev)
        assertNull(read.at)
        assertNull(read.wants)
        assertNull(read.part.tried)
    }
}
