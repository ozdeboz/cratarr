package app.cratarr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** What the app asks as it starts, read from the source: an activity does not run in a unit test. */
class AskedOnceTest {
    private val activity = File("src/main/java/app/cratarr/MainActivity.kt").readText().replace("\r\n", "\n")

    @Test
    fun notifications_are_asked_for_once_whatever_the_answer() {
        val ask = activity.substringAfter("private fun askForNotifications()").substringBefore("\n    }\n")
        assertTrue(ask, ask.contains("prefs.notificationsAsked"))
        assertTrue(ask, ask.contains("notificationsAsk.launch"))
        assertFalse("not the way with no answer to hear", ask.contains("requestPermissions"))
        val answered = activity.substringAfter("private val notificationsAsk = ").substringBefore('\n')
        assertTrue(answered, answered.contains("prefs.notificationsAsked = true"))
    }

    @Test
    fun the_app_says_how_it_stands_with_androids_battery_as_the_page_asks_by_its_background_row() {
        assertTrue(activity.contains(".put(\"batteryAllowed\""))
        assertTrue(activity.contains("\"battery\" -> if (message.optString(\"op\") == \"settings\") openAppSettings() else askToRunInBackground()"))
    }
}
