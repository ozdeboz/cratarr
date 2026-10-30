package app.cratarr

import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The announce mixer on the JVM (the owner, 2026-10-24: "We need better instrumentation"): a clip loud for a second and
 * silent to its end, mixed into steady music, 10 ms a buffer. Its sound is silence, so what comes out is the music at
 * the mixer's gain, read back sample by sample.
 */
class AnnouncerMixerTest {
    private val rate = 48_000
    private val channels = 2
    private val level = 10_000
    private val buffer = rate / 100

    /** A clip `ms` long whose loudness is speech for its first `loudMs`. */
    private fun clip(ms: Int, loudMs: Int): Announcer.Clip {
        val frames = rate * ms / 1000
        val block = rate * Announcer.BLOCK_MS / 1000
        val loudness = FloatArray((frames + block - 1) / block) { if (it * Announcer.BLOCK_MS < loudMs) 0.5f else 0f }
        return Announcer.Clip(ShortArray(frames * channels), channels, rate, loudness, block)
    }

    /** The music's gain at the end of each 10 ms buffer, for `ms`; `ended` set when the mixer says the clip is out. */
    private fun mix(m: Announcer.Mixer, ms: Int): FloatArray {
        val gains = FloatArray(ms / 10)
        for (i in gains.indices) {
            val inb = ByteBuffer.allocateDirect(buffer * channels * 2).order(ByteOrder.LITTLE_ENDIAN)
            repeat(buffer * channels) { inb.putShort(level.toShort()) }
            inb.flip()
            m.queueInput(inb)
            val out = m.output.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            gains[i] = out.get(out.limit() - 1) / level.toFloat()
        }
        return gains
    }

    private fun mixer(): Announcer.Mixer =
        Announcer.Mixer().also {
            it.configure(AudioProcessor.AudioFormat(rate, channels, C.ENCODING_PCM_16BIT))
            it.flush()
        }

    @Test
    fun the_music_goes_down_with_the_speech_and_up_after_it_and_the_clip_ends() {
        val m = mixer()
        var endedAt: Int? = null
        var buffers = 0
        val marks = mutableMapOf<Announcer.Mark, Long>()
        m.play(clip(2000, 1000), { what, at -> marks[what] = at }) { endedAt = buffers * 10 }
        val gains = FloatArray(500)
        for (i in gains.indices) {
            buffers = i + 1
            gains[i] = mix(m, 10)[0]
        }
        // Down at the duck within the attack (and a buffer), from the first speech.
        val down = gains.indexOfFirst { it <= Announcer.DUCK + 0.01f } * 10
        assertTrue("down at ${down} ms", down in 0..(Announcer.ATTACK_MS + 10))
        // Back to full within look-ahead + hold + release (and a buffer) after the last speech (at 1000 ms).
        val upBy = 1000 + Announcer.LOOK_AHEAD_MS + Announcer.HOLD_MS + Announcer.RELEASE_MS + 10
        val up = gains.indices.first { it * 10 > 1000 && gains[it] >= 0.999f } * 10
        assertTrue("up at ${up} ms, by ${upBy} expected", up <= upBy)
        assertTrue("still down at ${up} ms only after the speech's hold", up > 1000 + Announcer.HOLD_MS)
        // Out once the clip has played and the music is up: at its end, not at the give-up (10 s after it).
        assertNotNull("the clip never ended", endedAt)
        assertTrue("ended at ${endedAt} ms", endedAt!! <= 2000 + 10)
        // Its moments, in the clip's time.
        assertEquals(0L, marks[Announcer.Mark.STARTED])
        assertTrue("ducked mark ${marks[Announcer.Mark.DUCKED]}", (marks[Announcer.Mark.DUCKED] ?: -1) in 0..Announcer.ATTACK_MS.toLong() + 10)
        assertEquals(1000L, marks[Announcer.Mark.SPEECH_END])
        assertTrue("released mark ${marks[Announcer.Mark.RELEASED]}", (marks[Announcer.Mark.RELEASED] ?: Long.MAX_VALUE) <= upBy)
    }

    /** A spoken announcement: its words to within a block of its end, so the hold after them reaches past the end (the
     *  live bug, 2026-10-24: the music stayed down until the give-up). */
    @Test
    fun speech_to_the_clips_end_still_lets_the_music_up_and_the_clip_end() {
        val m = mixer()
        var endedAt: Int? = null
        var buffers = 0
        m.play(clip(1200, 1190)) { endedAt = buffers * 10 }
        val gains = FloatArray(400)
        for (i in gains.indices) {
            buffers = i + 1
            gains[i] = mix(m, 10)[0]
        }
        val upBy = 1190 + Announcer.LOOK_AHEAD_MS + Announcer.HOLD_MS + Announcer.RELEASE_MS + 10
        val up = gains.indices.firstOrNull { it * 10 > 1190 && gains[it] >= 0.999f }?.let { it * 10 }
        assertNotNull("the music never came back up", up)
        assertTrue("up at ${up} ms, by ${upBy} expected", up!! <= upBy)
        assertNotNull("the clip never ended", endedAt)
        assertTrue("ended at ${endedAt} ms, the music up at ${up}", endedAt!! <= up + 10)
    }

    @Test
    fun a_clip_given_up_lets_the_music_rise_at_the_release_not_at_once() {
        val m = mixer()
        m.play(clip(2000, 2000)) {}
        mix(m, 300)
        m.stop()
        val gains = mix(m, 700)
        assertTrue("rose at once: ${gains[0]}", gains[0] < 0.5f)
        val up = gains.indexOfFirst { it >= 0.999f } * 10
        assertTrue("up after ${up} ms", up in 1..(Announcer.RELEASE_MS + 10))
    }
}
