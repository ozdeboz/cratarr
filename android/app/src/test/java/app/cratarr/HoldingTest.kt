package app.cratarr

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.io.IOException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.UnknownHostException

class HoldingTest {
    private var tries = 0
    private val waiting = mutableListOf<Pair<Runnable, Long>>()
    private val holding = Holding({ r, ms -> waiting += r to ms }, { r -> waiting.removeAll { it.first === r } }) { tries++ }

    private val service = File("src/main/java/app/cratarr/PlayerService.kt").readText().replace("\r\n", "\n")
    private val link = File("src/main/java/app/cratarr/PlayOnLink.kt").readText().replace("\r\n", "\n")
    private val page = File("src/main/java/app/cratarr/PagePlayer.kt").readText().replace("\r\n", "\n")

    private fun timeComes() = waiting.removeAt(0).first.run()

    @After
    fun over() = holding.ended()

    @Test
    fun a_track_that_could_not_be_had_is_held_and_tried_again_in_a_while() {
        holding.begin()
        assertTrue(Holding.now)
        assertEquals(0, tries)
        assertEquals(listOf(Holding.AGAIN_MS), waiting.map { it.second })
        timeComes()
        assertEquals(1, tries)
    }

    @Test
    fun it_goes_on_trying_for_as_long_as_each_try_fails() {
        holding.begin()
        timeComes()
        holding.begin()
        timeComes()
        holding.begin()
        timeComes()
        assertEquals(3, tries)
        assertTrue(Holding.now)
    }

    @Test
    fun one_wait_at_a_time() {
        holding.begin()
        holding.begin()
        assertEquals(1, waiting.size)
    }

    @Test
    fun cratarr_answering_tries_at_once_and_leaves_no_wait_behind() {
        holding.begin()
        holding.answered()
        assertEquals(1, tries)
        assertTrue(waiting.isEmpty())
    }

    @Test
    fun cratarr_answering_with_nothing_held_asks_nothing_of_the_player() {
        holding.answered()
        assertEquals(0, tries)
    }

    @Test
    fun once_it_plays_nothing_more_is_tried() {
        holding.begin()
        holding.ended()
        assertFalse(Holding.now)
        assertTrue(waiting.isEmpty())
    }

    @Test
    fun a_wait_that_comes_after_the_end_tries_nothing() {
        holding.begin()
        val late = waiting[0].first
        holding.ended()
        late.run()
        assertEquals(0, tries)
    }

    @Test
    fun no_network_to_ask_on_is_cratarr_out_of_reach_too() {
        assertTrue(Patience.noWay(UnknownHostException("cratarr")))
        assertTrue(Patience.noWay(NoRouteToHostException()))
        assertTrue(Patience.noWay(ConnectException()))
        assertFalse(Patience.noWay(IOException("the disk")))
        assertFalse(Patience.noWay(null))
    }

    @Test
    fun out_of_reach_holds_where_the_queue_used_to_be_skipped_through() {
        val failed = service.substringAfter("val action = when {").substringBefore("LinkLog.add(")
        assertTrue(failed, failed.contains("Patience.outOfReach(cause) -> \"hold\""))
        assertTrue("before anything that moves on", failed.indexOf("\"hold\"") < failed.indexOf("\"skip\""))
        assertTrue(service.substringAfter("when (action) {").contains("\"hold\" -> holding.begin()"))
    }

    @Test
    fun the_try_is_the_players_own_from_where_it_stands() {
        val made = service.substringAfter("holding = Holding(").substringBefore("\n        )")
        assertTrue(made, made.contains("player.prepare()"))
        assertFalse(made, made.contains("seekTo"))
    }

    @Test
    fun paused_nothing_is_asked_by_itself_and_a_press_takes_it_from_there() {
        val made = service.substringAfter("holding = Holding(").substringBefore("\n        )")
        assertTrue(made, made.contains("player.playWhenReady"))
        val commands = File("src/main/java/app/cratarr/PlayerCommands.kt").readText().replace("\r\n", "\n")
        val resume = commands.substringAfter("fun resume(").substringBefore("\n    }\n")
        assertTrue(resume, resume.contains("Player.STATE_IDLE"))
    }

    @Test
    fun it_ends_as_the_player_is_ready_and_cratarr_answering_reaches_it() {
        assertTrue(service.contains("if (playbackState == Player.STATE_READY) holding.ended()"))
        assertTrue("nor outlasts the service", service.substringAfter("override fun onDestroy()").contains("holding.ended()"))
        assertTrue(service.substringAfter("fun cratarrAnswered()").substringBefore("\n    }\n").contains("holding.answered()"))
        assertTrue(link.substringAfter("opened = true").substringBefore("read(BufferedReader").contains("cratarrAnswered"))
    }

    @Test
    fun held_is_told_to_the_page_as_waiting_not_as_a_failure() {
        val report = page.substringAfter("fun report(withQueue").substringBefore("send(out.toString())")
        assertTrue(report, report.contains("StartStop.starting || (Holding.now && c.playWhenReady)"))
        assertTrue(report, report.contains("takeUnless { Holding.now }"))
    }
}
