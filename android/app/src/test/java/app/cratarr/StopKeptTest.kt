package app.cratarr

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Back after its music was handed away while it was out of reach, the app stops and keeps its queue to go on with later (the
 * owner, 2026-11-08: "Stop it, keep its queue"; the tablet, offline in a group it led, came back to an empty queue). Every
 * other stop of Cratarr's empties it, as before.
 */
class StopKeptTest {
    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")

    @Test
    fun a_kept_stop_pauses_with_the_queue_and_any_other_stop_of_cratarrs_empties_it() {
        val stop = source("PlayerCommands.kt").substringAfter("\"stop\" -> if (from == From.PAGE)").substringBefore("\"next\" ->")
        val ofCratarr = stop.substringAfter("} else {")
        assertTrue("a kept stop is told apart", ofCratarr.contains("if (m.optBoolean(\"kept\")) {"))
        val kept = ofCratarr.substringAfter("if (m.optBoolean(\"kept\")) {").substringBefore("} else {")
        assertTrue("kept: paused", kept.contains("p.pause()"))
        assertTrue("kept: nothing cleared", !kept.contains("clearMediaItems"))
        val other = ofCratarr.substringAfter("if (m.optBoolean(\"kept\")) {").substringAfter("} else {")
        assertTrue("any other: stopped and emptied", other.contains("p.stop()") && other.contains("p.clearMediaItems()"))
        assertTrue("the screen follows the music either way", ofCratarr.contains("followMusic(m)"))
    }
}
