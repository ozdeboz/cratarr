package app.cratarr

import android.view.KeyEvent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which media keys the app answers itself, on the device's own player, and which Media3 sends to the session's player. */
class MediaKeysTest {
    private val skipsAndWinds = listOf(
        KeyEvent.KEYCODE_MEDIA_PREVIOUS,
        KeyEvent.KEYCODE_MEDIA_NEXT,
        KeyEvent.KEYCODE_MEDIA_REWIND,
        KeyEvent.KEYCODE_MEDIA_FAST_FORWARD,
    )

    @Test
    fun skips_and_winds_are_the_apps_own_while_this_device_plays() {
        for (key in skipsAndWinds) assertTrue(MediaKeys.ours(key, driven = null, going = false))
    }

    @Test
    fun pointed_at_another_player_they_go_where_the_controls_point() {
        for (key in skipsAndWinds) assertFalse(MediaKeys.ours(key, driven = "Another player", going = false))
    }

    @Test
    fun a_key_going_when_the_controls_turned_away_stays_the_apps_until_let_go() {
        // Nothing else stops the wind it began on the device's own player.
        for (key in skipsAndWinds) assertTrue(MediaKeys.ours(key, driven = "Another player", going = true))
    }

    @Test
    fun play_and_pause_are_never_the_apps_own() {
        val others = listOf(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, KeyEvent.KEYCODE_MEDIA_PLAY, KeyEvent.KEYCODE_MEDIA_PAUSE, KeyEvent.KEYCODE_HEADSETHOOK)
        for (key in others) {
            assertFalse(MediaKeys.ours(key, driven = null, going = false))
            assertFalse(MediaKeys.ours(key, driven = "Another player", going = false))
            assertFalse(MediaKeys.ours(key, driven = null, going = true))
        }
    }
}
