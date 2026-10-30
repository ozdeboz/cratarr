package app.cratarr

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * A speed with the pitch following, and a volume, on the sound itself (winding in a sync group, decision 0074): a
 * player leading a group sends its sound on before Android's output, where its speed and volume otherwise act, so
 * leading a group they act here instead, before the sound is sent. A tape's way: the sound read faster or slower
 * (cubic between samples), both followed smoothly, never stepping. Untouched (1×, full) it passes the sound as it is.
 *
 * Where its player is ([mediaUs]): Android's output counts what it has played of what comes out here; read faster or
 * slower, that is more or less of what came in. 16-bit sound; other sound passes.
 */
@OptIn(UnstableApi::class)
class Varispeed : BaseAudioProcessor() {
    /** The speed (1: as it is), followed over [FOLLOW_MS]. */
    @Volatile
    var rate = 1f

    /** The volume (1: full), followed sample by sample, full scale in [SMOOTH_MS]. */
    @Volatile
    var gain = 1f

    private var format: AudioProcessor.AudioFormat = AudioProcessor.AudioFormat.NOT_SET
    private var channels = 1

    /** What came in and has not been read past yet, interleaved; its first frame is the one before [at]'s. */
    private var held = FloatArray(0)
    private var heldFrames = 0

    /** Where the next frame is read, in frames into [held]. */
    private var at = 1.0
    private var speed = 1f
    private var level = 1f

    /** Frames of what came in no longer held (since the last flush): [held]'s first frame is the one after them. */
    private var dropped = 0L

    /** Frames given out since the last flush, and how far into what came in they had read, at each buffer's end. */
    private var given = 0L
    private val marks = ArrayDeque<Pair<Long, Double>>()

    override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat): AudioProcessor.AudioFormat {
        format = inputAudioFormat
        channels = max(1, inputAudioFormat.channelCount)
        return inputAudioFormat
    }

    override fun onFlush(streamMetadata: AudioProcessor.StreamMetadata) = restart()

    override fun onReset() {
        restart()
        format = AudioProcessor.AudioFormat.NOT_SET
    }

    /** Afresh (a flush): nothing held, one silent frame before the first, nothing given or read. */
    @Synchronized
    private fun restart() {
        held = FloatArray(4 * channels)
        heldFrames = 1
        at = 1.0
        speed = rate
        level = gain
        dropped = 0L
        given = 0L
        marks.clear()
        marks.addLast(0L to 0.0)
    }

    /** Untouched: 1×, full, and nothing held back. */
    private fun plain(): Boolean = rate == 1f && speed == 1f && gain == 1f && level == 1f

    override fun queueInput(inputBuffer: ByteBuffer) {
        val n = inputBuffer.remaining()
        if (n == 0) return
        if (format.encoding != C.ENCODING_PCM_16BIT) {
            val out = replaceOutputBuffer(n)
            out.put(inputBuffer)
            out.flip()
            return
        }
        val inFrames = n / (2 * channels)
        if (plain()) {
            // Whatever was still held goes out first, from the nearest frame (half a sample at most, at 1×).
            val from = at.roundToInt().coerceIn(1, heldFrames)
            val rest = heldFrames - from
            val out = replaceOutputBuffer((rest + inFrames) * 2 * channels).order(ByteOrder.LITTLE_ENDIAN)
            for (i in from * channels until heldFrames * channels) out.putShort(clip(held[i]))
            // The last frame stays, the one before the next (a speed coming in reads from it).
            val last = inputBuffer.limit() - 2 * channels
            for (c in 0 until channels) held[c] = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).getShort(last + 2 * c).toFloat()
            out.put(inputBuffer)
            out.flip()
            dropped += heldFrames - 1 + inFrames
            given += rest + inFrames
            heldFrames = 1
            at = 1.0
            mark()
            return
        }
        // Held, then as much of what came as [CHUNK_MS] of sound at the speed asked reads (the rest is left for the next
        // call): slow, one buffer made seconds of sound at once, and a speed change waited for all of it to play
        // (2026-10-28, the tablet: Next's flying wind took three times its length).
        val rateNow = format.sampleRate.coerceAtLeast(1)
        val chunk = max(1, rateNow * CHUNK_MS / 1000)
        val wanted = (max(speed, rate.coerceIn(LEAST, MOST)) * chunk).toInt() + 4
        val take = min(inFrames, max(1, wanted - (heldFrames - floor(at).toInt())))
        val need = (heldFrames + take) * channels
        if (held.size < need) held = held.copyOf(max(need, held.size * 2))
        val sound = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        for (i in heldFrames * channels until need) held[i] = sound.get().toFloat()
        inputBuffer.position(inputBuffer.position() + take * 2 * channels)
        heldFrames += take
        val follow = (1 - exp(-1000.0 / (rateNow * FOLLOW_MS))).toFloat()
        val step = 1f / max(1, rateNow * SMOOTH_MS / 1000)
        // At most a chunk's worth.
        val most = min(((heldFrames - at) / LEAST + 2).toInt(), chunk) * channels
        val out = replaceOutputBuffer(most * 2).order(ByteOrder.LITTLE_ENDIAN)
        var made = 0
        val goal = rate.coerceIn(LEAST, MOST)
        val loud = gain
        while (floor(at).toInt() + 2 < heldFrames && made < most / channels) {
            val i = floor(at).toInt()
            val t = (at - i).toFloat()
            level = if (level < loud) min(loud, level + step) else max(loud, level - step)
            for (c in 0 until channels) {
                val y0 = held[(i - 1) * channels + c]
                val y1 = held[i * channels + c]
                val y2 = held[(i + 1) * channels + c]
                val y3 = held[(i + 2) * channels + c]
                out.putShort(clip(cubic(y0, y1, y2, y3, t) * level))
            }
            made++
            speed += (goal - speed) * follow
            if (abs(speed - goal) < 1e-4f) speed = goal
            at += speed
        }
        out.flip()
        // What was read past goes: one frame before where it reads is kept for the cubic.
        val drop = (floor(at).toInt() - 1).coerceIn(0, heldFrames)
        if (drop > 0) {
            System.arraycopy(held, drop * channels, held, 0, (heldFrames - drop) * channels)
            heldFrames -= drop
            at -= drop
            dropped += drop
        }
        given += made
        mark()
    }

    override fun onQueueEndOfStream() {
        // What is still held plays out as it is.
        if (format.encoding != C.ENCODING_PCM_16BIT || heldFrames <= 1) return
        val from = floor(at).toInt().coerceIn(1, heldFrames)
        val out = replaceOutputBuffer((heldFrames - from) * 2 * channels).order(ByteOrder.LITTLE_ENDIAN)
        for (i in from * channels until heldFrames * channels) out.putShort(clip(held[i]))
        out.flip()
        given += heldFrames - from
        System.arraycopy(held, (heldFrames - 1) * channels, held, 0, channels)
        dropped += heldFrames - 1
        heldFrames = 1
        at = 1.0
        mark()
    }

    /** Read so far (frames of what came in since the last flush): [held]'s first frame is the one before [at]'s. */
    private fun read(): Double = dropped + at - 1

    @Synchronized
    private fun mark() {
        marks.addLast(given to read())
        while (marks.size > MARKS) marks.removeFirst()
    }

    /**
     * Where its player is (µs since the last flush's start, as Android's output counts it, [playedUs] of sound played):
     * how much of what came in that sound read.
     */
    @Synchronized
    fun mediaUs(playedUs: Long): Long {
        val r = format.sampleRate
        if (r <= 0 || marks.isEmpty()) return playedUs
        val x = playedUs * r / 1_000_000.0
        // Marks behind what has been played are done with (one kept before it).
        while (marks.size >= 2 && marks.elementAt(1).first <= x) marks.removeFirst()
        val (g0, r0) = marks.first()
        val inFrames = if (marks.size >= 2 && x >= g0) {
            val (g1, r1) = marks.elementAt(1)
            r0 + (r1 - r0) * (x - g0) / (g1 - g0).coerceAtLeast(1)
        } else {
            r0 + (x - g0)
        }
        return (inFrames * 1_000_000.0 / r).toLong()
    }

    companion object {
        /** At most this much sound (ms) is made a call: a speed change acts within it. */
        private const val CHUNK_MS = 20
        /** The speed's change followed over this (ms): never a step. */
        private const val FOLLOW_MS = 10.0
        private const val SMOOTH_MS = 20
        /** Its range: Start and stop's and a wind's slowest, a held Next's fastest and more. */
        private const val LEAST = 0.05f
        private const val MOST = 8f
        private const val MARKS = 4096

        private fun clip(v: Float): Short = v.roundToInt().coerceIn(-32768, 32767).toShort()

        /** Catmull-Rom between [y1] and [y2], [t] of the way. */
        fun cubic(y0: Float, y1: Float, y2: Float, y3: Float, t: Float): Float =
            y1 + 0.5f * t * (y2 - y0 + t * (2f * y0 - 5f * y1 + 4f * y2 - y3 + t * (3f * (y1 - y2) + y3 - y0)))
    }
}
