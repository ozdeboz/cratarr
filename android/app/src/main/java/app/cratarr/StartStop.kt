package app.cratarr

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/**
 * Start and stop (Settings › This device; the owner, 2026-10-25), as the page's own player has them
 * (lib/localPlayer): Vinyl ramps the speed, the pitch following, as a turntable spins up and down; Fade the volume.
 * Over the device's ramp, eased. Natural track ends stay gapless: only play, pause and stop ramp, and next and previous
 * where the device asks: out only, the track changing at the end of it and the next one starting as it is (the owner,
 * 2026-10-26). Tape rewind: Previous restarting a track scrubs back to its start, pitched up, four times the ramp.
 *
 * Where it acts (the owner tried the first build, 2026-10-26: Vinyl broke into one odd speed, Fade ended early and
 * dropped, and came in with a plop): the speed on Android's own output (AudioTrack's playback rate, changed smoothly
 * and at once, never Media3's time-stretcher, which starts over at each change); the volume alone on the player's
 * volume, which also acts at once. Playing in a group (decision 0037) the leader's sound is sent on before both, so
 * Fade fades the sound itself there ([Fader]) and waits for the group's buffer before it pauses; Vinyl and Tape
 * rewind wait in a group (the owner: Vinyl later). Every volume follows a perceptual (dB) curve, and starts silent.
 *
 * The position holds through a ramp (the owner, 2026-10-26): a pause rests where it was pressed, its tail heard after
 * it; a play starts that much earlier ([lead]) so it is at full where it left off, the position shown held meanwhile.
 * Vinyl's speed eases in going down and out coming up, so the whole ramp is heard turning; a play from a track's
 * start never ramps.
 *
 * A ramp is not interrupted (the owner, 2026-10-26): while one runs, play, pause, next and previous are ignored, and
 * the state it commits to ([committed]) is what every control shows from its first moment: a pause says paused at once
 * while its sound winds down.
 */
class StartStop(
    private val player: ExoPlayer,
    private val prefs: Prefs,
    /** The player's volume worked out again ([gain] is part of it). */
    private val volume: () -> Unit,
    /** Whether it plays in a sync group now. */
    private val grouped: () -> Boolean,
    /** How long, playing in a group, until what is sent on is heard (its buffer). */
    private val groupDelayMs: () -> Long,
    /** What the controls show changed ([committed] set or let go): told to the session and to Cratarr. */
    private val changed: () -> Unit,
    /** The app's own log (LinkLog): what each press did, to follow a device's report. */
    private val log: (String, Array<Pair<String, Any?>>) -> Unit,
) {
    private val main = Handler(Looper.getMainLooper())
    private var token = 0

    /** While a ramp runs: whether it ends playing (true) or paused (false); null while none runs. */
    @Volatile
    var committed: Boolean? = null
        private set

    /** The position shown while a ramp runs (ms): where the press was; null otherwise. */
    @Volatile
    var heldPosition: Long? = null
        private set

    /** Whether a ramp (or Tape rewind) runs: presses wait for its end. */
    val busy: Boolean get() = committed != null

    private fun commit(to: Boolean?) {
        if (committed == to) return
        committed = to
        changed()
    }

    /** A ramp's direction told to the page (the play button draws it): `out`, `in`, or none at its end. */
    private fun ramping(dir: String?, ms: Long) {
        rampListener?.invoke(dir, ms)
    }

    private fun note(what: String, vararg fields: Pair<String, Any?>) =
        log(what, arrayOf<Pair<String, Any?>>("mode" to prefs.startStopMode, "playing" to player.isPlaying, "grouped" to grouped(), *fields))

    private fun mode(): String {
        val m = prefs.startStopMode
        return if (grouped() && m == Prefs.START_STOP_VINYL) Prefs.START_STOP_OFF else m
    }

    private fun ease(t: Float) = if (t < 0.5f) 2 * t * t else 1 - (-2 * t + 2) * (-2 * t + 2) / 2

    /** Steps [set] from [from] to [to] (0 still, 1 playing) over [ms], eased; then [done] (false: cut short). */
    /** A ramp's curve: Vinyl's speed eases in going down and out coming up (what the turntable does); Fade eases both. */
    private fun curve(mode: String, down: Boolean): (Float) -> Float = when {
        mode != Prefs.START_STOP_VINYL -> ::ease
        down -> { t -> t * t }
        else -> { t -> 1 - (1 - t) * (1 - t) }
    }

    /** How much earlier a play starts so it is at full where it left off: the ramp's length at its mean speed. */
    private fun lead(mode: String): Long =
        (prefs.startStopMs * if (mode == Prefs.START_STOP_VINYL) LOW + (1f - LOW) * 2f / 3f else 1f).toLong()

    private fun ramp(from: Float, to: Float, ms: Long, set: (Float) -> Unit, curve: (Float) -> Float, done: (Boolean) -> Unit) {
        val mine = ++token
        val t0 = SystemClock.elapsedRealtime()
        val step = object : Runnable {
            override fun run() {
                if (mine != token) return done(false)
                val t = ((SystemClock.elapsedRealtime() - t0).toFloat() / ms).coerceIn(0f, 1f)
                set(from + (to - from) * curve(t))
                if (t < 1f) main.postDelayed(this, STEP_MS) else done(true)
            }
        }
        step.run()
    }

    /** How far along (0 still, 1 playing) as the mode shows it. */
    private fun setter(mode: String): (Float) -> Unit = when {
        mode == Prefs.START_STOP_FADE && grouped() -> { v -> Fader.target = loudness(v) }
        mode == Prefs.START_STOP_FADE -> { v ->
            gain = loudness(v)
            volume()
        }
        // Vinyl: the speed with the pitch, from nearly still; the sound comes up over the first part of a spin-up and
        // goes over the last part of a spin-down, as a record's does.
        else -> { v ->
            val s = LOW + (1f - LOW) * v
            player.playbackParameters = PlaybackParameters(s, s)
            gain = loudness(min(1f, v / VINYL_SOUND))
            volume()
        }
    }

    /** Back to plain playing: full speed, its own pitch, full volume; a ramp under way stopped. */
    fun reset() {
        token++
        heldPosition = null
        if (committed != null) ramping(null, 0)
        commit(null)
        Fader.target = 1f
        if (gain != 1f) {
            gain = 1f
            volume()
        }
        if (player.playbackParameters != PlaybackParameters.DEFAULT) player.playbackParameters = PlaybackParameters.DEFAULT
    }

    /** Plays ([start]), winding up from still and silent. */
    fun play(start: () -> Unit) {
        note("play", "busy" to busy)
        if (busy) return
        val m = mode()
        val at = player.currentPosition
        // From a track's start: at once, always.
        if (m == Prefs.START_STOP_OFF || player.isPlaying || at < FROM_START_MS) {
            reset()
            start()
            return
        }
        val set = setter(m)
        // Silent before the first sound: no plop.
        set(0f)
        if (grouped()) Fader.snap()
        // Earlier by the ramp's lead, the position shown where it left off until it is there.
        heldPosition = at
        player.seekTo((at - lead(m)).coerceAtLeast(0))
        commit(true)
        ramping("in", prefs.startStopMs)
        start()
        ramp(0f, 1f, prefs.startStopMs, set, curve(m, down = false)) { whole -> if (whole) reset() }
    }

    /** Pauses or stops ([stop]), winding down first: once what was wound down has been heard. */
    fun pause(stop: () -> Unit) {
        note("pause", "busy" to busy)
        if (busy) return
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !player.isPlaying) {
            reset()
            stop()
            return
        }
        // Where it was pressed: shown meanwhile, and where it rests once paused.
        val at = player.currentPosition
        heldPosition = at
        commit(false)
        ramping("out", prefs.startStopMs)
        val mine = token + 1
        ramp(1f, 0f, prefs.startStopMs, setter(m), curve(m, down = true)) { whole ->
            if (!whole) return@ramp
            // A group hears what the leader sends a buffer later: silent meanwhile, then paused.
            val tail = if (grouped()) groupDelayMs() + GROUP_MARGIN_MS else 0L
            main.postDelayed({
                if (token == mine) {
                    stop()
                    if (player.playbackState != Player.STATE_IDLE && player.mediaItemCount > 0) player.seekTo(at)
                    reset()
                }
            }, tail)
        }
    }

    /** Next or previous ([go]) where the device asks it: out, then on, the next one as it is. */
    fun skip(go: () -> Unit) {
        note("skip", "busy" to busy, "skips" to prefs.startStopSkips)
        if (busy) return
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !prefs.startStopSkips || !player.isPlaying || grouped()) {
            reset()
            go()
            return
        }
        heldPosition = player.currentPosition
        commit(true)
        ramping("out", prefs.startStopMs)
        ramp(1f, 0f, prefs.startStopMs, setter(m), curve(m, down = true)) { whole ->
            if (!whole) return@ramp
            // Full speed and volume first: the next one starts as it is.
            reset()
            go()
        }
    }

    /**
     * Tape rewind: Previous that restarts the track scrubs back to its start, pitched up; false where it does not
     * (off, not playing, in a group, or early in the track, where Previous goes to the one before).
     */
    fun rewind(restartAfterMs: Long): Boolean {
        note("rewind", "busy" to busy, "rewind" to prefs.startStopRewind, "at_ms" to player.currentPosition)
        if (busy) return true
        if (!prefs.startStopRewind || grouped() || !player.isPlaying) return false
        val from = player.currentPosition
        if (from <= restartAfterMs) return false
        val ms = minOf(REWIND_MOST_MS, 4 * prefs.startStopMs)
        val mine = ++token
        commit(true)
        ramping("out", ms)
        val t0 = SystemClock.elapsedRealtime()
        player.playbackParameters = PlaybackParameters(REWIND_SPEED, REWIND_SPEED)
        gain = REWIND_GAIN
        volume()
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
        /** Told when a ramp starts (`out`, `in`, with its length) and ends (none): the page's player draws it. */
        @Volatile
        var rampListener: ((String?, Long) -> Unit)? = null

        /** The volume's share now (1: none), on the player's volume: `PlayOnLink.applyVolume` multiplies it in. */
        @Volatile
        var gain = 1f

        /** Perceptual loudness: 0 silent, 1 full, −60 dB at the quiet end and even in decibels between. */
        fun loudness(v: Float): Float = if (v <= 0f) 0f else 10f.pow(3f * (min(1f, v) - 1f))

        /** A play this near a track's start never ramps (the owner, 2026-10-26). */
        private const val FROM_START_MS = 500L

        /** Vinyl's slowest: Android's output takes the pitch down to a quarter, no lower. */
        private const val LOW = 0.25f

        /** Vinyl's sound over this first share of a spin-up (and last of a spin-down). */
        private const val VINYL_SOUND = 0.35f
        private const val STEP_MS = 16L
        private const val GROUP_MARGIN_MS = 250L
        private const val REWIND_SPEED = 2f
        /** Each step plays this long before the next seek back: shorter, and the seeks left nothing to hear. */
        private const val REWIND_STEP_MS = 300L
        private const val REWIND_MOST_MS = 12_000L
        private const val REWIND_GAIN = 0.8f
    }
}

/**
 * Fade on the sound itself (0.9.6), first in the player's chain (`SyncPlay.renderers`): a group's leader sends its
 * sound on before the volume, so this is how the whole group fades. Its gain follows [target] sample by sample, at most
 * full scale in [SMOOTH_MS], so it never steps. 16-bit sound is faded; other sound passes.
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
        val goal = target
        if ((level >= 0.999f && goal >= 0.999f) || format.encoding != C.ENCODING_PCM_16BIT) {
            level = goal
            out.put(inputBuffer)
        } else {
            val sound = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            val o = out.order(ByteOrder.LITTLE_ENDIAN)
            val channels = max(1, format.channelCount)
            val step = 1f / max(1, format.sampleRate * SMOOTH_MS / 1000)
            while (sound.remaining() >= channels) {
                level = if (level < goal) min(goal, level + step) else max(goal, level - step)
                for (c in 0 until channels) o.putShort((sound.get() * level).toInt().coerceIn(-32768, 32767).toShort())
            }
            while (sound.hasRemaining()) o.putShort(sound.get())
            inputBuffer.position(inputBuffer.limit())
        }
        out.flip()
    }

    companion object {
        /** What the sound is faded to (1: not at all). */
        @Volatile
        var target = 1f

        /** The level now, followed towards [target] sample by sample. */
        @Volatile
        private var level = 1f
        private const val SMOOTH_MS = 20

        /** At [target] at once (before a start: silent from its first sample). */
        fun snap() {
            level = target
        }
    }
}
