package app.cratarr

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class QueueKeptTest {
    private fun kept(ahead: Boolean, vararg ids: String) = JSONObject()
        .put("tracks", JSONArray(ids.map { JSONObject().put("source", "cratarr").put("id", it) }))
        .put("index", 0).put("position_ms", 1_000L).put(QueueKept.AHEAD, ahead).toString()

    private val source = File("src/main/java/app/cratarr/PlayerService.kt").readText().replace("\r\n", "\n")
    private val link = File("src/main/java/app/cratarr/PlayOnLink.kt").readText().replace("\r\n", "\n")

    @Test
    fun a_queue_ahead_of_cratarr_is_taken_up_as_the_player_starts() {
        assertTrue(QueueKept.atStart(kept(true, "tr-1")))
    }

    @Test
    fun a_queue_in_step_waits_for_cratarr_to_give_it_back_with_its_word_on_playing_on() {
        assertFalse(QueueKept.atStart(kept(false, "tr-1")))
    }

    @Test
    fun nothing_kept_or_nothing_that_can_be_read_is_no_queue() {
        assertFalse(QueueKept.atStart(""))
        assertFalse(QueueKept.atStart("{not json"))
        assertFalse(QueueKept.atStart(kept(true)))
        assertFalse(QueueKept.holds(""))
    }

    @Test
    fun cratarr_out_of_reach_a_player_with_nothing_takes_up_what_is_kept() {
        assertTrue(QueueKept.whenAway(kept(false, "tr-1"), 0, "@ann"))
        assertTrue(QueueKept.whenAway(kept(true, "tr-1"), 0, "@"))
    }

    @Test
    fun what_the_player_holds_is_not_replaced_by_what_is_kept() {
        assertFalse(QueueKept.whenAway(kept(false, "tr-1"), 1, "@ann"))
    }

    @Test
    fun nobody_signed_in_takes_up_no_cratarr_queue() {
        assertFalse(QueueKept.whenAway(kept(false, "tr-1"), 0, Who.NONE))
    }

    @Test
    fun nothing_kept_is_nothing_to_take_up() {
        assertFalse(QueueKept.whenAway("", 0, "@ann"))
    }

    @Test
    fun a_queue_is_ahead_until_a_report_with_it_has_got_through() {
        assertTrue("reports failing", QueueKept.aheadNow(through = false, queueSent = true))
        assertTrue("the queue not in one yet", QueueKept.aheadNow(through = true, queueSent = false))
        assertFalse(QueueKept.aheadNow(through = true, queueSent = true))
    }

    @Test
    fun each_mode_keeps_its_queue_as_it_goes_cratarrs_too_under_whose_it_is() {
        // Read from the source: a Media3 player does not run in a unit test.
        val keep = source.substringAfter("private val keepQueue = Runnable").substringBefore('\n')
        assertTrue(keep, keep.contains("prefs.saveQueue(prefs.mode, snapshot().toString(), queueWho)"))
        assertFalse(keep, keep.contains("ownQueue"))
        val kept = source.substringAfter("override fun onEvents(player: Player, events: Player.Events) {").substringBefore("Each time a track is reached")
        assertTrue(kept.contains("main.postDelayed(keepQueue, 1_000)"))
        assertFalse("not for one mode only", kept.substringBefore("main.postDelayed(keepQueue, 1_000)").substringAfterLast("if (").contains("ownQueue"))
    }

    @Test
    fun the_place_is_kept_while_it_plays_as_a_kill_leaves_no_last_word() {
        val count = source.substringAfter("private val count = object : Runnable").substringBefore("override fun onEvents")
        assertTrue(count.contains("keepPlace()"))
    }

    @Test
    fun a_kept_queue_holds_its_tracks_whole_so_a_play_from_keeps_its_from() {
        // `queueOf` without `thin`: every field of the track, as the page or Cratarr gave it, `from` among them.
        val snapshot = source.substringAfter("private fun snapshot(): JSONObject").substringBefore("/** A queue put by")
        assertTrue(snapshot, snapshot.contains(".put(\"tracks\", queueOf(player))"))
        assertTrue(snapshot, snapshot.contains("QueueKept.AHEAD"))
        val track = JSONObject().put("entry", "e").put("source", "cratarr").put("id", "tr-1").put("from", "cratarr//in")
        assertEquals("cratarr//in", copyNamed(JSONObject(track.toString())))
    }

    @Test
    fun cratarrs_mode_takes_up_what_is_kept_at_the_start_and_when_the_link_fails() {
        val start = source.substringAfter("override fun onCreate()").substringBefore("Who.changed.add(whoChanged)")
        assertTrue(start.contains("QueueKept::atStart"))
        assertTrue(source.substringAfter("fun cratarrAway()").substringBefore("\n    }\n").contains("QueueKept.whenAway"))
        assertTrue(link.substringAfter("if (!opened) {").substringBefore("val held =").contains("cratarrAway"))
    }

    @Test
    fun a_report_that_told_cratarr_of_the_queue_says_the_copy_is_in_step_again() {
        val sent = link.substringAfter("val sent = put(\"\$base/state\", body)").substringBefore("}\n    }\n")
        assertTrue(sent, sent.contains("queueSent = true"))
        assertTrue(sent, sent.contains("service.queueTold()"))
    }
}
