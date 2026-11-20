package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The device's Back (BACK): the page asked, its answer taken once and only for its own press, the WebView's history after the wait.
 * DeviceBack is pure; the activity's wiring is read from its source (the JVM tests cannot run a WebView).
 */
class DeviceBackTest {
    @Test
    fun thePageWentBackAndTheAppDoesNothingMore() {
        val b = DeviceBack()
        val id = b.ask()
        assertEquals(DeviceBack.Answer.WENT, b.answered(id, true))
        assertFalse("answered: no fallback after", b.timedOut(id))
    }

    @Test
    fun thePageShowsNoBackAndTheAppGoesToTheBackground() {
        val b = DeviceBack()
        assertEquals(DeviceBack.Answer.NONE, b.answered(b.ask(), false))
    }

    @Test
    fun unansweredItFallsBackOnceAndALateAnswerIsIgnored() {
        val b = DeviceBack()
        val id = b.ask()
        assertTrue(b.timedOut(id))
        assertFalse("once", b.timedOut(id))
        assertNull("the page's answer after the fallback changes nothing", b.answered(id, false))
    }

    @Test
    fun anAnswerToAnEarlierBackIsNotThisOnes() {
        val b = DeviceBack()
        val first = b.ask()
        val second = b.ask()
        assertNull(b.answered(first, false))
        assertFalse("the first's wait is over", b.timedOut(first))
        assertEquals(DeviceBack.Answer.WENT, b.answered(second, true))
        assertNull("no number, no Back", DeviceBack().answered(0, false))
    }

    @Test
    fun theActivityAsksThePageAndFallsBackAfterItsWait() {
        val activity = File("src/main/java/app/cratarr/MainActivity.kt").readText().replace("\r\n", "\n")
        assertTrue(activity.contains("val p = page ?: return backByHistory()"))
        assertTrue(activity.contains("p.postMessage(JSONObject().put(\"type\", \"back\").put(\"id\", id).toString())"))
        assertTrue(activity.contains("backWait.postDelayed({ if (back.timedOut(id)) backByHistory() }, DeviceBack.WAIT_MS)"))
        assertTrue(activity.contains("\"back\" -> if (back.answered(message.optInt(\"id\"), message.optBoolean(\"done\")) == DeviceBack.Answer.NONE) moveTaskToBack(true)"))
        assertFalse("never straight to the background past the page's Back", activity.contains("if (view != null && view.canGoBack()) view.goBack() else moveTaskToBack(true)\n                }"))
        assertEquals(1_000L, DeviceBack.WAIT_MS)
    }
}
