package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * What the app wires of a device answering for itself. The unit tests have no rules library (D04), so these read the source: that a line
 * is there, and that what the rule decides is no longer decided here. The rule's own tests are the rules crate's, the bridge's its own.
 */
class DeviceAnswersTest {
    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")
    private fun page(name: String) = File("../../web/src/lib/$name").readText().replace("\r\n", "\n")

    @Test
    fun when_autoplay_asks_and_how_many_are_the_rules() {
        val radio = source("Radio.kt")
        assertTrue(radio.contains("autoplayDue(count.toUInt(), c.toUInt())"))
        assertTrue(radio.contains("autoplayBatch()"))
        assertFalse("no number of its own", radio.contains("const val LOW") || radio.contains("const val BATCH"))
    }

    @Test
    fun where_radios_inserts_go_is_the_rules() {
        val radio = source("Radio.kt")
        assertTrue(radio.contains("radioPlace("))
        assertFalse("no walk of its own over the lanes", radio.contains("== \"next\") at++"))
    }

    @Test
    fun the_next_round_of_repeat_all_is_the_rules() {
        val repeat = source("Repeat.kt")
        assertTrue(repeat.contains("nextRound(queue,"))
        assertFalse("no shuffle of its own", repeat.contains(".shuffle()"))
        assertFalse("no places of its own", repeat.contains("sortedMapOf"))
    }

    @Test
    fun with_cratarr_away_both_lanes_ask_the_device_then_the_rule_for_what_is_kept() {
        val radio = source("Radio.kt")
        assertEquals("Radio and Autoplay alike", 3, radio.split("Scorer.offline(app, ask, queue").size)
        assertTrue(radio.contains("DeviceAnswers.pick(context, ask, here)"))
        assertTrue(radio.contains("picksFromKept("))
        assertFalse("no pick of its own", radio.contains(".shuffled()"))
    }

    @Test
    fun what_plays_here_is_what_is_downloaded_of_cratarrs_own() {
        val offline = source("Radio.kt").substringAfter("fun offline(").substringBefore("fun tracksOf(")
        assertTrue(offline.contains("optString(\"state\") == \"done\""))
        assertTrue(offline.contains("optString(\"source\") == CRATARR"))
    }

    @Test
    fun cratarr_answering_lets_the_copy_go() {
        val ask = source("Radio.kt").substringAfter("fun ask(").substringBefore("fun offline(")
        assertTrue(ask.contains("if (ok) DeviceAnswers.release()"))
    }

    @Test
    fun the_copy_is_read_anew_when_what_is_kept_moved_or_another_account_came() {
        val answers = source("DeviceAnswers.kt")
        assertTrue(answers.contains("it.who == who && it.stands == stands"))
        assertTrue("the old one let go first: two would not fit", answers.indexOf("release()") < answers.indexOf("deviceOpen("))
    }

    @Test
    fun plays_not_told_go_with_every_asking() {
        val ask = source("DeviceAnswers.kt").substringAfter("private fun ask(").substringBefore("fun pick(")
        assertTrue(ask.contains("Plays.waiting(app)"))
        assertTrue("before what is asked", ask.indexOf("\"plays\"") < ask.indexOf("device.ask(asked"))
    }

    @Test
    fun plays_of_another_account_are_not_this_ones() {
        assertTrue(source("Plays.kt").substringAfter("fun waiting(").substringBefore("/** Told").contains("ours(it, who)"))
    }

    @Test
    fun the_page_asks_where_the_app_answers() {
        assertTrue(page("deviceAnswers.svelte.ts").contains("const APP_ANSWER = '/__device/answer'"))
        assertTrue(source("DeviceAnswers.kt").contains("const val PAGE_PATH = \"/__device/answer\""))
        assertTrue(source("CratarrWebViewClient.kt").contains("if (url.path == DeviceAnswers.PAGE_PATH) return DeviceAnswers.answer(view.context, url.getQueryParameter(\"ask\"))"))
        assertTrue("the service worker leaves it alone, as all of it", source("BundledPage.kt").contains("\"/__device\""))
    }

    @Test
    fun a_device_with_no_rules_library_goes_on_with_what_it_has() {
        for (name in listOf("Radio.kt", "Repeat.kt", "DeviceAnswers.kt")) assertTrue(name, source(name).contains("catch (e: LinkageError)"))
    }
}
