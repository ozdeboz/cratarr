package app.cratarr

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Start and stop (Settings › This device; the owner, 2026-10-25), as the page's own player has them
 * (lib/localPlayer): Vinyl ramps the speed, the pitch following, as a turntable spins up and down; Fade the volume.
 * Over the device's ramp, eased; a new one cuts one under way short. Natural track ends stay gapless: only play,
 * pause and stop ramp, and next and previous where the device asks. Tape rewind: Previous restarting a track scrubs
 * back to its start, pitched up, four times the ramp. In a group (decision 0037) the sound its leader sends on is taken
 * before the speed: Fade, on the sound itself ([Fader]), fades the whole group; Vinyl and Tape rewind wait (the owner,
 * 2026-10-26: Vinyl later).
 */
class StartStop(
    private val player: ExoPlayer,
    private val prefs: Prefs,
    /** Whether it plays in a sync group now. */
    private val grouped: () -> Boolean,
) {
    private val main = Handler(Looper.getMainLooper())
    private var token = 0

    private fun mode(): String {
        val m = prefs.startStopMode
        return if (grouped() && m == Prefs.START_STOP_VINYL) Prefs.START_STOP_OFF else m
    }

    private fun ease(t: Float) = if (t < 0.5f) 2 * t * t else 1 - (-2 * t + 2) * (-2 * t + 2) / 2

    /** Steps [set] from [from] to [to] over [ms], eased; then [done] (false: cut short by another). */
    private fun ramp(from: Float, to: Float, ms: Long, set: (Float) -> Unit, done: (Boolean) -> Unit) {
        val mine = ++token
        val t0 = SystemClock.elapsedRealtime()
        val step = object : Runnable {
            override fun run() {
                if (mine != token) return done(false)
                val t = ((SystemClock.elapsedRealtime() - t0).toFloat() / ms).coerceIn(0f, 1f)
                set(from + (to - from) * ease(t))
                if (t < 1f) main.postDelayed(this, STEP_MS) else done(true)
            }
        }
        step.run()
    }

    private fun setter(mode: String): (Float) -> Unit =
        if (mode == Prefs.START_STOP_FADE) {
            { v -> gain = v }
        } else {
            { v ->
                val s = v.coerceAtLeast(LOW)
                player.playbackParameters = PlaybackParameters(s, s)
            }
        }

    private fun still(mode: String) = if (mode == Prefs.START_STOP_FADE) 0f else LOW

    /** Back to plain playing: full speed, its own pitch, full volume; a ramp under way stopped. */
    fun reset() {
        token++
        gain = 1f
        if (player.playbackParameters != PlaybackParameters.DEFAULT) player.playbackParameters = PlaybackParameters.DEFAULT
    }

    /** Plays ([start]), winding up from still (or silence). */
    fun play(start: () -> Unit) {
        val m = mode()
        if (m == Prefs.START_STOP_OFF || player.isPlaying) {
            reset()
            start()
            return
        }
        val set = setter(m)
        set(still(m))
        start()
        ramp(still(m), 1f, prefs.startStopMs, set) { whole -> if (whole) reset() }
    }

    /** Pauses or stops ([stop]), winding down first. */
    fun pause(stop: () -> Unit) {
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !player.isPlaying) {
            reset()
            stop()
            return
        }
        ramp(1f, still(m), prefs.startStopMs, setter(m)) { whole ->
            if (whole) {
                stop()
                reset()
            }
        }
    }

    /** Next or previous ([go]) where the device asks it: out, on, in. */
    fun skip(go: () -> Unit) {
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !prefs.startStopSkips || !player.isPlaying) {
            reset()
            go()
            return
        }
        val set = setter(m)
        ramp(1f, still(m), prefs.startStopMs, set) { whole ->
            if (!whole) return@ramp
            go()
            set(still(m))
            ramp(still(m), 1f, prefs.startStopMs, set) { done -> if (done) reset() }
        }
    }

    /**
     * Tape rewind: Previous that restarts the track scrubs back to its start, pitched up; false where it does not
     * (off, not playing, in a group, or early in the track, where Previous goes to the one before).
     */
    fun rewind(restartAfterMs: Long): Boolean {
        if (!prefs.startStopRewind || grouped() || !player.isPlaying) return false
        val from = player.currentPosition
        if (from <= restartAfterMs) return false
        val ms = minOf(REWIND_MOST_MS, 4 * prefs.startStopMs)
        val mine = ++token
        val t0 = SystemClock.elapsedRealtime()
        player.playbackParameters = PlaybackParameters(2f, 2f)
        gain = REWIND_GAIN
        val step = object : Runnable {
            override fun run() {
                if (mine != token) return
                val t = ((SystemClock.elapsedRealtime() - t0).toFloat() / ms).coerceIn(0f, 1f)
                player.seekTo((from * (1 - ease(t))).toLong())
                if (t < 1f) {
                    main.postDelayed(this, REWIND_STEP_MS)
                } else {
                    player.seekTo(0)
                    reset()
                }
            }
        }
        step.run()
        return true
    }

    companion object {
        /** Fade's share of the sound now (1: none); [Fader] applies it. */
        @Volatile
        var gain = 1f

        /** Vinyl's slowest: nearly still. */
        private const val LOW = 0.08f
        private const val STEP_MS = 16L
        private const val REWIND_STEP_MS = 90L
        private const val REWIND_MOST_MS = 12_000L
        private const val REWIND_GAIN = 0.8f
    }
}

/**
 * Fade's gain on the sound itself (0.9.6), first in the player's chain (`SyncPlay.renderers`): what a group's leader
 * sends on to the group fades with it, and alone it fades the same way. 16-bit sound is faded; other sound passes.
 */
@OptIn(UnstableApi::class)
class Fader : BaseAudioProcessor() {
    @Volatile private var format: AudioProcessor.AudioFormat = AudioProcessor.AudioFormat.NOT_SET

    override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat): AudioProcessor.AudioFormat {
        format = inputAudioFormat
        return inputAudioFormat
    }

    override fun queueInput(inputBuffer: ByteBuffer) {
        val n = inputBuffer.remaining()
        if (n == 0) return
        val out = replaceOutputBuffer(n)
        val g = StartStop.gain
        if (g >= 0.999f || format.encoding != C.ENCODING_PCM_16BIT) {
            out.put(inputBuffer)
        } else {
            val sound = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            val o = out.order(ByteOrder.LITTLE_ENDIAN)
            while (sound.hasRemaining()) o.putShort((sound.get() * g).toInt().coerceIn(-32768, 32767).toShort())
            inputBuffer.position(inputBuffer.limit())
        }
        out.flip()
    }
}
