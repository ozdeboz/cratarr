package app.cratarr

import android.app.ApplicationExitInfo
import android.content.ComponentCallbacks2
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the app writes down of the device: which lines are due, and what they say. */
class DeviceWatchTest {
    private val status = "Name:\tapp.cratarr\nVmRSS:\t  236196 kB\nRssAnon:\t   77916 kB\nVmSwap:\t       0 kB\n"

    @Test
    fun a_value_is_read_by_its_name_alone() {
        assertEquals(236_196L, DeviceWatch.kbOf(status, "VmRSS"))
        assertEquals(0L, DeviceWatch.kbOf(status, "VmSwap"))
        assertNull(DeviceWatch.kbOf(status, "RssFile"))
        assertNull(DeviceWatch.kbOf(status, "Vm"))
    }

    @Test
    fun a_line_is_due_first_on_a_large_change_and_now_and_then() {
        val mb = 1024L
        assertEquals("first", DeviceWatch.due(null, 100 * mb, 0))
        assertNull(DeviceWatch.due(100 * mb, 160 * mb, 60_000))
        assertEquals("changed", DeviceWatch.due(100 * mb, 164 * mb, 60_000))
        assertEquals("changed", DeviceWatch.due(164 * mb, 100 * mb, 60_000))
        assertNull(DeviceWatch.due(100 * mb, 100 * mb, 29 * 60_000))
        assertEquals("regular", DeviceWatch.due(100 * mb, 100 * mb, 30 * 60_000))
    }

    @Test
    fun a_page_is_its_route_without_what_the_route_holds() {
        assertEquals("crates", DeviceWatch.pageOf("http://192.0.2.1:8081/#/crates"))
        assertEquals("crates", DeviceWatch.pageOf("http://192.0.2.1:8081/#/crates/7?sort=year"))
        assertEquals("home", DeviceWatch.pageOf("http://192.0.2.1:8081/#home?words=a"))
        assertNull(DeviceWatch.pageOf("http://192.0.2.1:8081/"))
        assertNull(DeviceWatch.pageOf("http://192.0.2.1:8081/#/"))
    }

    @Test
    fun a_shortage_is_worth_a_line_and_going_down_the_list_is_not() {
        assertEquals("running critical", DeviceWatch.trimWord(ComponentCallbacks2.TRIM_MEMORY_RUNNING_CRITICAL))
        assertEquals("page left", DeviceWatch.trimWord(ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN))
        assertNull(DeviceWatch.trimWord(ComponentCallbacks2.TRIM_MEMORY_BACKGROUND))
    }

    @Test
    fun an_end_is_named_and_only_a_harmless_one_is_quiet() {
        assertEquals("user requested", DeviceWatch.reasonWord(ApplicationExitInfo.REASON_USER_REQUESTED))
        assertEquals("freezer", DeviceWatch.reasonWord(ApplicationExitInfo.REASON_FREEZER))
        assertEquals("unknown 99", DeviceWatch.reasonWord(99))
        assertTrue(DeviceWatch.quietEnd(ApplicationExitInfo.REASON_PACKAGE_UPDATED))
        assertFalse(DeviceWatch.quietEnd(ApplicationExitInfo.REASON_USER_REQUESTED))
        assertFalse(DeviceWatch.quietEnd(ApplicationExitInfo.REASON_LOW_MEMORY))
    }

    @Test
    fun music_out_of_the_foreground_is_told_once_past_the_grace_and_again_in_a_new_spell() {
        val u = DeviceWatch.Unprotected(30_000)
        assertFalse(u.playing(false, 0))
        assertFalse(u.playing(false, 29_999))
        assertTrue(u.playing(false, 30_000))
        assertFalse(u.playing(false, 60_000))
        u.stopped()
        assertFalse(u.playing(false, 70_000))
        assertTrue(u.playing(false, 100_000))
    }

    @Test
    fun the_foreground_reached_in_time_tells_nothing() {
        val u = DeviceWatch.Unprotected(30_000)
        assertFalse(u.playing(false, 0))
        assertFalse(u.playing(true, 5_000))
        assertFalse(u.playing(false, 31_000))
        assertFalse(u.playing(false, 60_000))
    }
}
