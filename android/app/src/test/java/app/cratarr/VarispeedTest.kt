package app.cratarr

import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The speed on the sound itself (winding in a sync group): untouched it passes the sound as it is; slower it makes more
 * of it and says how much of the track that was; a speed coming and going never clicks.
 */
class VarispeedTest {
    private val rate = 48_000
    private val channels = 2
    private val buffer = rate / 100

    /** One at [speed] from its first sample (set before it starts, as the tape's is). */
    private fun varispeed(speed: Float = 1f): Varispeed = Varispeed().also {
        it.rate = speed
        it.configure(AudioProcessor.AudioFormat(rate, channels, C.ENCODING_PCM_16BIT))
        it.flush()
    }

    /** A 440 Hz tone, frame [from] on, [frames] long, both channels. */
    private fun tone(from: Int, frames: Int): ShortArray = ShortArray(frames * channels) {
        (10_000 * sin(2 * PI * 440 * (from + it / channels) / rate)).toInt().toShort()
    }

    /** One buffer in, as a pipeline gives it (again until it is all taken), what came out. */
    private fun run(v: Varispeed, sound: ShortArray): ShortArray {
        val inb = ByteBuffer.allocateDirect(sound.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        sound.forEach { inb.putShort(it) }
        inb.flip()
        val got = ArrayList<Short>()
        do {
            v.queueInput(inb)
            val out = v.output.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            while (out.hasRemaining()) got.add(out.get())
        } while (inb.hasRemaining())
        return got.toShortArray()
    }

    @Test
    fun untouched_it_passes_the_sound_as_it_is() {
        val v = varispeed()
        for (i in 0 until 50) {
            val s = tone(i * buffer, buffer)
            assertArrayEquals(s, run(v, s))
        }
        assertEquals(500_000L, v.mediaUs(500_000L))
    }

    @Test
    fun at_half_speed_it_makes_twice_the_sound_and_its_place_is_half_as_far() {
        val v = varispeed(0.5f)
        var made = 0
        for (i in 0 until 100) made += run(v, tone(i * buffer, buffer)).size / channels
        // A second in, two out (but the two frames held for what comes next).
        assertEquals(2.0 * rate, made.toDouble(), 4.0)
        // One second of it played is half a second of the track.
        assertEquals(500_000.0, v.mediaUs(1_000_000L).toDouble(), 2_000.0)
    }

    @Test
    fun a_place_counted_on_is_read_as_it_was_made_and_never_past_what_was_given() {
        // Half a second made at a quarter, then half a second at 1× (a wind's speed asked up while the output still
        // plays what was made before): what the output plays is read as it was made, whatever is asked now.
        val v = varispeed(0.25f)
        var made = 0
        while (made < rate / 2) made += run(v, tone(made, buffer)).size / channels
        v.rate = 1f
        var given = made
        while (given < rate) given += run(v, tone(given, buffer)).size / channels
        // 250 ms played: all of it from the quarter's sound, a sixteenth of a second of the track.
        assertEquals(62_500.0, v.mediaUsAhead(250_000L).toDouble(), 2_000.0)
        assertEquals(0.25, v.madeSpeedAt(250_000L), 0.01)
        // Counted on does not drop the marks a later, earlier word needs.
        assertEquals(62_500.0, v.mediaUsAhead(250_000L).toDouble(), 2_000.0)
        // 800 ms played: what was made at a quarter (whole buffers: a little over half a second), then the rest at 1×.
        val quarterUs = made * 1_000_000.0 / rate
        // Less the speed's own following up from the quarter (over 10 ms, never a step): three quarters of 10 ms.
        assertEquals(quarterUs * 0.25 + (800_000.0 - quarterUs) - 7_500.0, v.mediaUsAhead(800_000L).toDouble(), 2_000.0)
        assertEquals(1.0, v.madeSpeedAt(800_000L), 0.02)
        // Never past what was given: counted on far beyond it, it stays at the end of what was made.
        val end = v.mediaUsAhead(given * 1_000_000L / rate)
        assertEquals(end, v.mediaUsAhead(10_000_000L))
    }

    @Test
    fun a_speed_coming_and_going_never_clicks() {
        val v = varispeed()
        val out = ArrayList<Short>()
        for (i in 0 until 300) {
            // 1×, down to a quarter, up to four, back to 1×: a wind's way and a held Next's.
            v.rate = when (i) {
                in 50 until 100 -> 0.25f
                in 100 until 150 -> 4f
                else -> 1f
            }
            v.gain = if (i in 200 until 220) 0f else 1f
            run(v, tone(i * buffer, buffer)).forEachIndexed { k, s -> if (k % channels == 0) out.add(s) }
        }
        // A 440 Hz tone at 10 000 moves at most ~580 a sample at 1×, ~2300 at 4×: nothing near a click's jump.
        var most = 0
        for (k in 1 until out.size) most = maxOf(most, abs(out[k] - out[k - 1]))
        assertTrue("largest step $most", most < 3_000)
        // Back at 1× and full, the sound passes as it is again: the tone, at the place it has reached.
        val tail = run(v, tone(300 * buffer, buffer))
        assertEquals(buffer * channels, tail.size)
    }
}
