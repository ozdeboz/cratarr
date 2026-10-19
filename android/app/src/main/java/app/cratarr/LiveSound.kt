/*
 * The sound as it is heard, for the page's visuals (decision 0046, amended 2026-10-19: in the app the music plays
 * natively, so the page has no sound of its own to listen to). The page's live analysis (web lib/visuals/live.ts) keeps
 * its running statistics; the app gives it what its analysers would: each tick (12 ms of sound), each band's energy and
 * flux from a 46 ms spectrum (as an AnalyserNode: Blackman window, |X|/N, no smoothing), and about every 100 ms the
 * mean square of the last 400 ms through R128's K-weighting filters (as the page's biquads). Worked out as the player
 * hands its sound over (a tee in its audio path, after decoding), and sent when it is heard, about twenty times a
 * second: `{type: "live", at, tickMs, channels, meanSquare, ticks: [{energy, flux}], wave}`, `wave` the last tick's
 * [WAVE] samples mixed to one, as bytes centred on 128 (an AnalyserNode's byte time domain; decision 0056's Waves
 * source), in base64. Only while the page asks.
 */
package app.cratarr

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Base64
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import org.json.JSONArray
import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.ln
import kotlin.math.log2
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

@OptIn(UnstableApi::class)
object LiveSound : TeeAudioProcessor.AudioBufferSink {
    /** Each band's edges (Hz; 0 open), as live.ts's LIVE_EDGES. */
    private val EDGES = arrayOf(0.0 to 60.0, 60.0 to 150.0, 150.0 to 400.0, 400.0 to 2000.0, 2000.0 to 5000.0, 5000.0 to 0.0)
    const val TICK_MS = 12
    /** The waveform's samples a tick. */
    const val WAVE = 512
    /** Magnitudes compressed as ln(1 + γ·|X|) before the flux (live.ts's GAMMA). */
    private const val GAMMA = 100.0
    /** Ticks heard go to the page this often at most. */
    private const val SEND_MS = 50L
    /** Ticks not yet heard kept at most (a full sink holds about a second). */
    private const val PENDING_MAX = 1000

    /** Who gets the ticks (the page, through MainActivity, on the main thread); none: nothing is worked out. */
    @Volatile var listener: ((String) -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())

    // The player's thread from here on: the tee, the sink's position and its flush all come from it.
    private var rate = 0
    private var channels = 0
    private var encoding = C.ENCODING_INVALID
    /** The buffer the sink is writing (its presentation time) and how many frames of it the tee has seen. */
    private var bufferAtUs = Long.MIN_VALUE
    private var bufferFrames = 0L

    private var n = 2048
    private var ring = FloatArray(n)
    private var ringAt = 0
    private var hop = 1
    private var sinceTick = 0
    private var window = DoubleArray(n)
    private var band = IntArray(n / 2)
    private var previous = DoubleArray(n / 2)
    private val re = DoubleArray(4096)
    private val im = DoubleArray(4096)

    // K-weighting (a high shelf, then a high pass) and the last 400 ms of its squares.
    private var shelf = Biquad.NONE
    private var pass = Biquad.NONE
    private var squares = DoubleArray(1)
    private var squaresAt = 0
    private var squaresSum = 0.0
    private var squaresSeen = 0
    private var meanSquare: Double? = null
    private var sinceMean = 0

    private class Tick(val atUs: Long, val energy: DoubleArray, val flux: DoubleArray, val wave: ByteArray)

    private val pending = ArrayDeque<Tick>()
    private val ready = ArrayList<Tick>()
    private var sentAt = 0L

    /** The sink takes a buffer that plays at [presentationTimeUs] (the tee then sees its sound). */
    fun writing(presentationTimeUs: Long) {
        if (presentationTimeUs != bufferAtUs) {
            bufferAtUs = presentationTimeUs
            bufferFrames = 0
        }
    }

    /** A seek, a jump or a stop: what was waiting is not heard. */
    fun clear() {
        pending.clear()
        ready.clear()
        previous.fill(0.0)
        bufferAtUs = Long.MIN_VALUE
    }

    override fun flush(sampleRateHz: Int, channelCount: Int, encoding: Int) {
        clear()
        if (sampleRateHz == rate && channelCount == channels && encoding == this.encoding) return
        rate = sampleRateHz
        channels = channelCount
        this.encoding = encoding
        if (rate <= 0) return
        // About 46 ms, as the page's analyser and the server.
        n = if (rate > 64_000) 4096 else 2048
        ring = FloatArray(n)
        ringAt = 0
        hop = max(1, (rate * TICK_MS / 1000.0).roundToInt())
        sinceTick = 0
        window = DoubleArray(n) { i -> 0.42 - 0.5 * cos(2 * PI * i / n) + 0.08 * cos(4 * PI * i / n) }
        band = IntArray(n / 2) { k -> if (k == 0) -1 else EDGES.indexOfFirst { (lo, hi) -> val hz = k.toDouble() * rate / n; hz >= lo && (hi == 0.0 || hz < hi) } }
        previous = DoubleArray(n / 2)
        shelf = Biquad.highShelf(rate, 1681.97, 3.99984)
        // The page's biquad takes its Q in dB for a high pass (Web Audio's rule): the same here.
        pass = Biquad.highPass(rate, 38.135, 10.0.pow(0.5003 / 20))
        squares = DoubleArray(1 shl log2(rate * 0.4).roundToInt())
        squaresAt = 0
        squaresSum = 0.0
        squaresSeen = 0
        meanSquare = null
        sinceMean = 0
    }

    override fun handleBuffer(buffer: ByteBuffer) {
        if (listener == null || rate <= 0 || channels <= 0) return
        val b = buffer.order(ByteOrder.LITTLE_ENDIAN)
        val bytes = when (encoding) {
            C.ENCODING_PCM_16BIT -> 2
            C.ENCODING_PCM_24BIT -> 3
            C.ENCODING_PCM_32BIT, C.ENCODING_PCM_FLOAT -> 4
            else -> return
        }
        val frames = b.remaining() / (bytes * channels)
        for (f in 0 until frames) {
            // Mixed to one, as the page's analyser does.
            var sum = 0.0
            for (c in 0 until channels) sum += sample(b, bytes)
            val x = sum / channels
            ring[ringAt] = x.toFloat()
            ringAt = (ringAt + 1) % n
            val k = pass.next(shelf.next(x))
            squaresSum += k * k - squares[squaresAt]
            squares[squaresAt] = k * k
            squaresAt = (squaresAt + 1) % squares.size
            // Summed again at each round, so the running sum does not drift.
            if (squaresAt == 0) squaresSum = squares.sum()
            squaresSeen = minOf(squaresSeen + 1, squares.size)
            if (++sinceMean >= rate / 10) {
                sinceMean = 0
                meanSquare = if (squaresSeen == squares.size) max(0.0, squaresSum / squares.size) else null
            }
            if (++sinceTick >= hop) {
                sinceTick = 0
                val atUs = if (bufferAtUs == Long.MIN_VALUE) Long.MIN_VALUE else bufferAtUs + (bufferFrames + f + 1) * 1_000_000L / rate
                if (atUs != Long.MIN_VALUE && pending.size < PENDING_MAX) pending.add(tick(atUs))
            }
        }
        bufferFrames += frames
    }

    /** The sink is heard at [positionUs] (the player asks it often while it plays): the ticks heard by now go out. */
    fun heard(positionUs: Long) {
        val to = listener ?: return
        while (pending.isNotEmpty() && pending.first().atUs <= positionUs) ready.add(pending.removeFirst())
        val now = SystemClock.elapsedRealtime()
        if (ready.isEmpty() || now - sentAt < SEND_MS) return
        sentAt = now
        val ticks = JSONArray()
        for (t in ready) ticks.put(JSONObject().put("energy", numbers(t.energy)).put("flux", numbers(t.flux)))
        val message = JSONObject()
            .put("type", "live")
            // When the first of them was heard (epoch milliseconds), the next ones a tick apart.
            .put("at", System.currentTimeMillis() - (ready.size - 1) * TICK_MS)
            .put("tickMs", TICK_MS)
            .put("channels", channels)
            .put("meanSquare", meanSquare ?: JSONObject.NULL)
            .put("ticks", ticks)
            .put("wave", Base64.encodeToString(ready.last().wave, Base64.NO_WRAP))
            .toString()
        ready.clear()
        main.post { to(message) }
    }

    /** One tick: the spectrum of the last [n] samples, each band's energy and flux. */
    private fun tick(atUs: Long): Tick {
        for (i in 0 until n) {
            re[i] = ring[(ringAt + i) % n] * window[i]
            im[i] = 0.0
        }
        fft(n)
        val energy = DoubleArray(EDGES.size)
        val flux = DoubleArray(EDGES.size)
        for (k in 1 until n / 2) {
            val m = sqrt(re[k] * re[k] + im[k] * im[k]) / n
            val y = ln(1 + GAMMA * m)
            val rise = max(0.0, y - previous[k])
            previous[k] = y
            val b = band[k]
            if (b < 0) continue
            energy[b] += m * m
            flux[b] += rise
        }
        // The last [WAVE] samples, as bytes centred on 128.
        val wave = ByteArray(WAVE) { i -> (ring[(ringAt + n - WAVE + i) % n] * 128 + 128).roundToInt().coerceIn(0, 255).toByte() }
        return Tick(atUs, energy, flux, wave)
    }

    private fun sample(b: ByteBuffer, bytes: Int): Double = when (encoding) {
        C.ENCODING_PCM_16BIT -> b.short / 32768.0
        C.ENCODING_PCM_24BIT -> {
            val lo = b.get().toInt() and 0xff
            val mid = b.get().toInt() and 0xff
            val hi = b.get().toInt()
            ((hi shl 16) or (mid shl 8) or lo) / 8388608.0
        }
        C.ENCODING_PCM_32BIT -> b.int / 2147483648.0
        C.ENCODING_PCM_FLOAT -> b.float.toDouble()
        else -> 0.0.also { b.position(b.position() + bytes) }
    }

    /** Numbers as the page reads them: a float's precision is plenty. */
    private fun numbers(values: DoubleArray): JSONArray = JSONArray().apply { for (v in values) put(v.toFloat().toDouble().takeIf { it.isFinite() } ?: 0.0) }

    /** In place, on [re] and [im] (radix 2). */
    private fun fft(size: Int) {
        var j = 0
        for (i in 1 until size) {
            var bit = size shr 1
            while (j and bit != 0) {
                j = j xor bit
                bit = bit shr 1
            }
            j = j or bit
            if (i < j) {
                re[i] = re[j].also { re[j] = re[i] }
                im[i] = im[j].also { im[j] = im[i] }
            }
        }
        var len = 2
        while (len <= size) {
            val angle = -2 * PI / len
            val wr = cos(angle)
            val wi = sin(angle)
            var i = 0
            while (i < size) {
                var cr = 1.0
                var ci = 0.0
                for (k in 0 until len / 2) {
                    val a = i + k
                    val b = a + len / 2
                    val tr = re[b] * cr - im[b] * ci
                    val ti = re[b] * ci + im[b] * cr
                    re[b] = re[a] - tr
                    im[b] = im[a] - ti
                    re[a] += tr
                    im[a] += ti
                    val nr = cr * wr - ci * wi
                    ci = cr * wi + ci * wr
                    cr = nr
                }
                i += len
            }
            len = len shl 1
        }
    }

    /** A biquad filter (the Audio EQ Cookbook's, as Web Audio's), sample by sample. */
    private class Biquad(private val b0: Double, private val b1: Double, private val b2: Double, private val a1: Double, private val a2: Double) {
        private var x1 = 0.0
        private var x2 = 0.0
        private var y1 = 0.0
        private var y2 = 0.0

        fun next(x: Double): Double {
            val y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
            x2 = x1
            x1 = x
            y2 = y1
            y1 = y
            return y
        }

        companion object {
            val NONE = Biquad(1.0, 0.0, 0.0, 0.0, 0.0)

            fun highShelf(rate: Int, f0: Double, gainDb: Double): Biquad {
                val a = 10.0.pow(gainDb / 40)
                val w = 2 * PI * f0 / rate
                val (c, s) = cos(w) to sin(w)
                // Web Audio's shelves: a slope of 1.
                val alpha = s / 2 * sqrt(2.0)
                val ra = 2 * sqrt(a) * alpha
                val a0 = (a + 1) - (a - 1) * c + ra
                return Biquad(
                    a * ((a + 1) + (a - 1) * c + ra) / a0,
                    -2 * a * ((a - 1) + (a + 1) * c) / a0,
                    a * ((a + 1) + (a - 1) * c - ra) / a0,
                    2 * ((a - 1) - (a + 1) * c) / a0,
                    ((a + 1) - (a - 1) * c - ra) / a0,
                )
            }

            fun highPass(rate: Int, f0: Double, q: Double): Biquad {
                val w = 2 * PI * f0 / rate
                val (c, s) = cos(w) to sin(w)
                val alpha = s / (2 * q)
                val a0 = 1 + alpha
                return Biquad((1 + c) / 2 / a0, -(1 + c) / a0, (1 + c) / 2 / a0, -2 * c / a0, (1 - alpha) / a0)
            }
        }
    }
}
