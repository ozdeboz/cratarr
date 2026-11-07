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
import java.io.File
import java.io.IOException

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

    private inner class StandIn : IndexRule {
        var says: ((SyncPart) -> SyncStep)? = null
        val starts = mutableListOf<String>()
        var network: SyncNetwork? = null
        var mobile: SyncMobile? = null

        override fun step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, now: Long): SyncStep {
            this.network = network
            this.mobile = mobile
            return says?.invoke(part) ?: when {
                part.stopped -> SyncStep.Stopped
                part.lost > 0u || part.failed > 0u -> SyncStep.Wait(now + 1)
                part.whole -> SyncStep.Whole
                else -> SyncStep.Changes
            }
        }

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

        override fun takeWhole(kind: SyncKind, bytes: ByteArray) =
            if (bytes.contentEquals(otherVersion)) SyncTaken(SyncOutcome.VERSION, null, null) else SyncTaken(SyncOutcome.TOOK, 5L, null)

        override fun takeChanges(kind: SyncKind, kept: ByteArray, feed: ByteArray) =
            if (feed.contentEquals(badFeed)) SyncTaken(SyncOutcome.START_AGAIN, null, null) else SyncTaken(SyncOutcome.TOOK, 9L, kept + feed)
    }

    private val rule = StandIn()
    private val asked = mutableListOf<String>()
    /** How Cratarr answers each address, by its path without `?`. */
    private val answers = mutableMapOf<String, () -> IndexFetched>()
    private var who = "@ann"
    private var clock = 1_000L
    private lateinit var files: IndexFiles
    private lateinit var looks: IndexLooks

    @Before
    fun made() {
        files = IndexFiles(folder.root)
        looks = IndexLooks(rule, files, ::fetch, { who }, { SyncNetwork.METERED }, { SyncMobile.YES }, { clock })
    }

    private fun fetch(path: String): IndexFetched {
        asked.add(path)
        answers[path.substringBefore('?')]?.let { return it() }
        return when (path.substringBefore('?')) {
            "/api/index" -> IndexFetched(200, whole)
            "/api/index/sounds" -> IndexFetched(200, sounds)
            else -> IndexFetched(204, null)
        }
    }

    private fun bytes(name: String, of: String = who) = files.bytes(of, name).readBytes()

    /** Both parts held, and a new start over them: as an app opened again leaves it. */
    private fun held() {
        looks.look(who)
        asked.clear()
        clock += 1_000
        looks.startAnew()
    }

    @Test
    fun whole_keeps_the_bytes_as_fetched_each_part_at_its_rev_and_dated() {
        clock = 2_000
        looks.look(who)
        assertEquals(listOf("/api/index", "/api/index/sounds"), asked)
        assertArrayEquals(whole, bytes("index"))
        assertArrayEquals(sounds, bytes("sounds"))
        val stands = files.stands(who, "index")!!
        assertEquals(5L, stands.rev)
        assertEquals(2_000L, stands.at)
        assertFalse(stands.part.whole)
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
        assertEquals(listOf("/api/index/changes?since=5", "/api/index/sounds/changes?since=5"), asked)
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
        assertEquals(listOf("/api/index/changes?since=5", "/api/index", "/api/index/sounds/changes?since=5"), asked)
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
        assertEquals(listOf("plain", "plain"), rule.starts)
    }

    @Test
    fun a_start_over_what_another_version_kept_is_told_as_that_and_the_rule_says_what_it_means() {
        held()
        rule.starts.clear()
        val stands = files.stands(who, "index")!!
        files.keep(who, "index", IndexStands(stands.rev, stands.at, 2, stands.part), null)
        looks.look(who)
        assertEquals(listOf("by another", "plain"), rule.starts)
        assertEquals(listOf("/api/index", "/api/index/sounds/changes?since=5"), asked)
    }

    @Test
    fun a_part_is_taken_from_what_is_kept_once_a_start() {
        held()
        rule.starts.clear()
        looks.look(who)
        looks.look(who)
        assertEquals(2, rule.starts.size)
    }

    @Test
    fun a_start_takes_each_part_from_what_is_kept_again() {
        held()
        looks.look(who)
        rule.starts.clear()
        looks.startAnew()
        looks.look(who)
        assertEquals(2, rule.starts.size)
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
        assertEquals(listOf("/api/index", "/api/index/sounds"), asked)
        assertTrue("it waits as any failure does", steps["index"] is SyncStep.Wait)
        assertNull(files.stands(who, "index"))
    }

    @Test
    fun a_rule_that_keeps_asking_ends_with_the_look() {
        rule.says = { SyncStep.Whole }
        answers["/api/index"] = { IndexFetched(500, null) }
        answers["/api/index/sounds"] = { IndexFetched(500, null) }
        looks.look(who)
        assertEquals(6, asked.size)
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
        assertEquals(listOf(true, true), facts.map { it.asks })
        assertEquals(listOf(true, true), facts.map { it.whole })
    }

    @Test
    fun the_page_is_told_each_part_as_the_look_left_it() {
        answers["/api/index"] = { IndexFetched(200, otherVersion) }
        val facts = looks.facts(looks.look(who))
        assertEquals(listOf(true, false), facts.map { it.stopped })
        assertEquals(listOf(false, false), facts.map { it.asks })
    }

    @Test
    fun paused_is_the_rules_to_say() {
        held()
        answers["/api/index/changes"] = { IndexFetched(null, null) }
        rule.says = { if (it.lost < 3u) SyncStep.Changes else SyncStep.Wait(0) }
        val facts = looks.facts(looks.look(who))
        assertEquals(listOf(true, false), facts.map { it.paused })
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
        assertTrue(source("Reach.kt").contains("checkSoon(app, SETTLE_MS) { DeviceIndex.look(app) }"))
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
            assertTrue(type, sync.contains("tellApp('$type')"))
            assertTrue(type, activity.contains("\"$type\" -> "))
        }
    }

    @Test
    fun what_the_app_tells_is_what_the_page_listens_for() {
        val sync = page("deviceSync.svelte.ts")
        val told = source("DeviceIndex.kt").substringAfter("fun report(").substringBefore("fun answer(")
        assertTrue(sync.contains("onApp('device-index',"))
        assertTrue(told.contains(".put(\"type\", \"device-index\")"))
        for (field in listOf("at", "rev", "metered", "parts")) assertTrue(field, told.contains(".put(\"$field\","))
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
        assertTrue(source("CratarrWebViewClient.kt").contains("if (url.path == DeviceIndex.PAGE_PATH) deviceIndex()?.let { return it }"))
        assertTrue(source("MainActivity.kt").contains("deviceIndex = { DeviceIndex.answer(app) }"))
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
