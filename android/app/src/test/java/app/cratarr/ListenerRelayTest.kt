package app.cratarr

import androidx.media3.common.Player
import androidx.media3.common.Timeline
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Every event of the player reaches whoever a wrapper stands in front of: a session behind one that dropped them had no controls. */
class ListenerRelayTest {
    @Test
    fun it_hands_on_every_method_of_the_listener() {
        val missing = Player.Listener::class.java.methods.filter { m ->
            runCatching { ListenerRelay::class.java.getDeclaredMethod(m.name, *m.parameterTypes) }.isFailure
        }
        assertTrue("not handed on: ${missing.map { it.name }}", missing.isEmpty())
    }

    @Test
    fun an_event_reaches_the_listener_behind_it_and_a_subclass_changes_only_its_own() {
        val heard = mutableListOf<String>()
        val behind = object : Player.Listener {
            override fun onTimelineChanged(timeline: Timeline, reason: Int) { heard += "timeline" }
            override fun onPlaybackStateChanged(playbackState: Int) { heard += "state $playbackState" }
            override fun onIsPlayingChanged(isPlaying: Boolean) { heard += "playing $isPlaying" }
        }
        val relay = object : ListenerRelay(behind) {
            override fun onIsPlayingChanged(isPlaying: Boolean) = super.onIsPlayingChanged(true)
        }
        relay.onTimelineChanged(Timeline.EMPTY, Player.TIMELINE_CHANGE_REASON_PLAYLIST_CHANGED)
        relay.onPlaybackStateChanged(Player.STATE_READY)
        relay.onIsPlayingChanged(false)
        assertEquals(listOf("timeline", "state ${Player.STATE_READY}", "playing true"), heard)
    }
}
