package app.cratarr

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
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
 * 2026-10-26). Tape rewind (0.9.9): Previous restarting a track plays it backwards to its start, as a tape rewinds, four
 * times the ramp: a clip Cratarr renders (`/api/stream/{source}/{id}/rewind`), on a small player of its own ([rewind]).
 *
 * Where it acts (the owner tried the first build, 2026-10-26: Vinyl broke into one odd speed, Fade ended early and
 * dropped, and came in with a plop): the speed on Android's own output (AudioTrack's playback rate, changed smoothly
 * and at once, never Media3's time-stretcher, which starts over at each change); the volume alone on the player's
 * volume, which also acts at once. Playing in a group (decision 0037) the leader's sound is sent on before both, so
 * Fade fades the sound itself there ([Fader]) and waits for the group's buffer before it pauses; Vinyl waits in a
 * group (the owner: later). Every volume follows a perceptual (dB) curve, and starts silent.
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
    private val context: Context,
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
    /** Tape rewind's clip of the playing track, from a moment (ms) back to its start, as the device's controls shape it;
     *  none for a track Cratarr has no file of. */
    private val rewindUrl: (Long) -> String?,
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

    /** Tape rewind's player, made the first time; its clip's listener while one plays or is coming, whether it has
     *  started (the track then waits at its start), and what restarts the track as ever. */
    private var tape: ExoPlayer? = null
    private var tapeListener: Player.Listener? = null
    private var tapeStarted = false
    private var tapeRestart: (() -> Unit)? = null
    /** Where the clip began (ms into the track), how long it is, and the share spinning up and braking take: the way
     *  back the position shown follows. */
    private var tapeFrom = 0L
    private var tapeMs = 1L
    private var tapeRamp = 0.2f

    /** When Previous was pressed (elapsed realtime): every step of a rewind is logged in ms since (0.9.11, the owner: a
     *  rewind took far longer than its clip; where the time goes). The track's player watched meanwhile. */
    private var tapeT0 = 0L
    private var tapeTicks = 0
    private var trackWatch: Player.Listener? = null

    private fun step(what: String, vararg fields: Pair<String, Any?>) =
        note("rewind", "step" to what, "ms" to SystemClock.elapsedRealtime() - tapeT0, *fields)

    private fun stateName(s: Int) = when (s) {
        Player.STATE_IDLE -> "idle"
        Player.STATE_BUFFERING -> "buffering"
        Player.STATE_READY -> "ready"
        Player.STATE_ENDED -> "ended"
        else -> "$s"
    }

    /** The track's player through a rewind and until it plays again from the start (then let go). */
    private fun watchTrack() {
        trackWatch?.let(player::removeListener)
        val w = object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) = step("track ${stateName(state)}", "at_ms" to player.currentPosition)

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                step(if (isPlaying) "track playing" else "track not playing", "at_ms" to player.currentPosition)
                if (isPlaying && tapeListener == null) {
                    player.removeListener(this)
                    if (trackWatch === this) trackWatch = null
                }
            }
        }
        trackWatch = w
        player.addListener(w)
    }

    /** Tape rewind's clip sounds now: the position shown goes back to the start with it (0.9.10). */
    val rewinding: Boolean get() = tapeListener != null && tapeStarted

    /** The position shown while the clip sounds, told on as it goes back. */
    private val tapeTick = object : Runnable {
        override fun run() {
            val t = tape ?: return
            if (!rewinding) return
            heldPosition = rewoundTo(t.currentPosition)
            // Its pace against the clock, once a second.
            if (tapeTicks++ % 4 == 0) step("clip pace", "clip_ms" to t.currentPosition, "buffered_ms" to t.bufferedPosition)
            changed()
            main.postDelayed(this, REWIND_TICK_MS)
        }
    }

    /**
     * Where the way back is [clipMs] into the clip, at its overall pace (the server's speed curve: eased up over the
     * ramp's share, flat, eased down; its cuts left out, the owner 2026-10-27): the curve's area so far over the whole.
     */
    private fun rewoundTo(clipMs: Long): Long {
        val f = tapeRamp.toDouble()
        val u = (clipMs.toDouble() / tapeMs).coerceIn(0.0, 1.0)
        val eased = { x: Double -> (x - f / Math.PI * kotlin.math.sin(Math.PI * x / f)) / 2 }
        val whole = 1 - f
        val area = when {
            u <= f -> eased(u)
            u <= 1 - f -> f / 2 + (u - f)
            else -> whole - eased(1 - u)
        }
        return (tapeFrom * (1 - area / whole)).toLong().coerceAtLeast(0L)
    }
    private val tapeLate = Runnable {
        if (tapeListener != null && !tapeStarted) {
            step("clip late")
            rewound()
        }
    }

    /** The ramp under way (`in`, `out`; none), where it is (0 still, 1 playing), and a pause's own stop, waiting for
     *  its ramp's end: what a seek in the middle of one needs (`seek`). */
    private var running: String? = null
    private var level = 1f
    private var stopping: (() -> Unit)? = null
    /** A press the other way that came during a ramp (0.9.8: a remote stop during a wind-up was dropped, the old file
     *  playing on): run once the ramp is over, as a press then would. */
    private var after: (() -> Unit)? = null

    private fun commit(to: Boolean?) {
        if (committed == to) return
        committed = to
        changed()
    }

    /** A ramp's direction told to the page (the play button draws it): `out`, `in`, or none at its end. */
    private fun ramping(dir: String?, ms: Long, full: Long = ms) {
        running = dir
        rampListener?.invoke(dir, ms, full)
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

    /** A ramp's mean speed as a share of full: Vinyl's spin-up runs slower than the music, a fade at full speed. */
    private fun pace(mode: String): Float = if (mode == Prefs.START_STOP_VINYL) LOW + (1f - LOW) * 2f / 3f else 1f

    /** How long a wind-up from [at] (ms into the track) is: as set, but only as long as there is track before it to
     *  start early on (0.9.7, the owner: a 3 s fade from 1 s in is a 1 s one); none from its very start. */
    private fun windUp(mode: String, at: Long): Long =
        if (at < FROM_START_MS) 0L else min(prefs.startStopMs, (at / pace(mode)).toLong())

    private fun ramp(from: Float, to: Float, ms: Long, set: (Float) -> Unit, curve: (Float) -> Float, done: (Boolean) -> Unit) {
        val mine = ++token
        val t0 = SystemClock.elapsedRealtime()
        val step = object : Runnable {
            override fun run() {
                if (mine != token) return done(false)
                val t = ((SystemClock.elapsedRealtime() - t0).toFloat() / ms).coerceIn(0f, 1f)
                level = from + (to - from) * curve(t)
                set(level)
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
        stopTape()
        token++
        val next = after
        after = null
        next?.let { main.post(it) }
        heldPosition = null
        stopping = null
        level = 1f
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
        if (busy) {
            // During a wind-down: played once it is over; during a wind-up, it plays already.
            if (committed == false) after = { play(start) }
            return
        }
        val m = mode()
        val at = player.currentPosition
        // From a track's start: at once, always.
        val ms = windUp(m, at)
        if (m == Prefs.START_STOP_OFF || player.isPlaying || ms <= 0L) {
            reset()
            start()
            return
        }
        val set = setter(m)
        // Silent before the first sound: no plop.
        set(0f)
        if (grouped()) Fader.snap()
        // Earlier by the ramp's lead (never before the track's start), the position shown where it left off until it is
        // there; the page told how long it is of the length set, so its button fills from where that leaves it.
        heldPosition = at
        player.seekTo((at - (ms * pace(m)).toLong()).coerceAtLeast(0))
        commit(true)
        ramping("in", ms, prefs.startStopMs)
        start()
        ramp(0f, 1f, ms, set, curve(m, down = false)) { whole -> if (whole) reset() }
    }

    /** Pauses or stops ([stop]), winding down first: once what was wound down has been heard. */
    fun pause(stop: () -> Unit) {
        note("pause", "busy" to busy)
        if (busy) {
            // During a wind-up: wound down once it is over; during a wind-down, it stops already.
            if (committed == true) after = { pause(stop) }
            return
        }
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !player.isPlaying) {
            reset()
            stop()
            return
        }
        // Where it was pressed: shown meanwhile, and where it rests once paused.
        val at = player.currentPosition
        heldPosition = at
        stopping = stop
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

    /**
     * A seek ([go]) in the middle of a ramp wins at once, with no gap added and no click (0.9.7, the owner; as the web's
     * player): during a wind-up (or a skip's wind-down), the jump at once, then speed and volume from where the ramp was
     * to full over [SETTLE_MS]; during a pause's wind-down, [SILENCE_MS] to silence, paused, resting where it was sent.
     * Either way the ramp ends now and the position is no longer held. Without a ramp, a seek as ever.
     */
    fun seek(go: () -> Unit) {
        // During Tape rewind: the clip stops, the track goes where it was sent and plays on.
        if (tapeListener != null) {
            note("seek", "during" to "rewind")
            val started = stopTape()
            reset()
            go()
            if (started) player.play()
            return
        }
        val dir = running
        if (!busy || dir == null) return go()
        val m = mode()
        val set = setter(m)
        val from = level
        val stop = stopping
        heldPosition = null
        stopping = null
        ramping(null, 0)
        commit(null)
        if (dir == "out" && stop != null) {
            note("seek", "during" to "pause")
            ramp(from, 0f, SILENCE_MS, set, { it }) { whole ->
                if (!whole) return@ramp
                stop()
                go()
                reset()
            }
        } else {
            note("seek", "during" to dir)
            go()
            ramp(from, 1f, SETTLE_MS, set, { it }) { whole -> if (whole) reset() }
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
     * Tape rewind: Previous that restarts the track plays it backwards to its start, then the track from there; false
     * where it does not (off, not playing, in a group, early in the track, where Previous goes to the one before, or a
     * track Cratarr has no file of). The clip plays on a player of its own, asking no focus (the track's player holds
     * it), while the track waits at its start, paused and buffered; one not ready within [REWIND_WAIT_MS], or failing,
     * and the track restarts as ever ([restart]).
     */
    fun rewind(restartAfterMs: Long, restart: () -> Unit): Boolean {
        note("rewind", "busy" to busy, "rewind" to prefs.startStopRewind, "at_ms" to player.currentPosition)
        if (busy) return true
        if (!prefs.startStopRewind || grouped() || !player.isPlaying) return false
        val from = player.currentPosition
        if (from <= restartAfterMs) return false
        val ms = prefs.rewindMs
        val url = rewindUrl(from) ?: return false
        tapeT0 = SystemClock.elapsedRealtime()
        tapeTicks = 0
        step("pressed", "from_ms" to from, "clip_ms" to ms, "url" to url.substringAfter("/api/stream/"))
        watchTrack()
        tapeFrom = from
        tapeMs = ms.coerceAtLeast(1L)
        tapeRamp = prefs.rewindRamp
        heldPosition = from
        commit(true)
        val t = tape ?: ExoPlayer.Builder(context)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), false)
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(context, Downloads.http(context))))
            .build()
            .also { tape = it }
        val listener = object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (tapeListener !== this) return
                step("clip ${stateName(state)}", "clip_ms" to t.currentPosition, "length_ms" to t.duration.takeIf { it > 0 })
                if (state == Player.STATE_READY && !tapeStarted) {
                    tapeStarted = true
                    main.removeCallbacks(tapeLate)
                    ramping("out", ms)
                    player.pause()
                    player.seekTo(0)
                    t.play()
                    main.post(tapeTick)
                } else if (state == Player.STATE_ENDED) {
                    rewound()
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (tapeListener !== this) return
                step(if (isPlaying) "clip playing" else "clip not playing", "clip_ms" to t.currentPosition)
            }

            override fun onPlayerError(error: PlaybackException) {
                if (tapeListener !== this) return
                step("clip failed", "error" to error.errorCodeName, "why" to error.message)
                rewound()
            }
        }
        tapeListener = listener
        tapeStarted = false
        tapeRestart = restart
        t.addListener(listener)
        t.volume = player.volume
        t.setMediaItem(MediaItem.fromUri(url))
        t.playWhenReady = false
        t.prepare()
        step("clip prepared")
        main.postDelayed(tapeLate, REWIND_WAIT_MS)
        return true
    }

    /** Tape rewind over (its clip ended, failed or came too late): the track from its start. */
    private fun rewound() {
        val restart = tapeRestart
        val started = stopTape()
        step(if (started) "track from its start" else "track restarted as ever", "track_state" to stateName(player.playbackState), "at_ms" to player.currentPosition)
        reset()
        if (started) player.play() else restart?.invoke()
    }

    /** Stops Tape rewind's clip, if one plays or is coming; whether it had started. */
    private fun stopTape(): Boolean {
        val l = tapeListener ?: return false
        tapeListener = null
        tapeRestart = null
        main.removeCallbacks(tapeLate)
        main.removeCallbacks(tapeTick)
        tape?.let {
            it.removeListener(l)
            it.stop()
            it.clearMediaItems()
        }
        return tapeStarted.also { tapeStarted = false }
    }

    /** Tape rewind's player let go (the service ends). */
    fun release() {
        stopTape()
        tape?.release()
        tape = null
    }

    companion object {
        /** Told when a ramp starts (`out`, `in`, with its length and the length set) and ends (none): the page's player
         *  draws it. */
        @Volatile
        var rampListener: ((String?, Long, Long) -> Unit)? = null

        /** The volume's share now (1: none), on the player's volume: `PlayOnLink.applyVolume` multiplies it in. */
        @Volatile
        var gain = 1f

        /** Perceptual loudness: 0 silent, 1 full, −60 dB at the quiet end and even in decibels between. */
        fun loudness(v: Float): Float = if (v <= 0f) 0f else 10f.pow(3f * (min(1f, v) - 1f))

        /** A seek in a wind-up: speed and volume back to full over this; in a pause's wind-down: silent over this. */
        private const val SETTLE_MS = 30L
        private const val SILENCE_MS = 8L

        /** A play this near a track's start never ramps (the owner, 2026-10-26). */
        private const val FROM_START_MS = 500L

        /** Vinyl's slowest: Android's output takes the pitch down to a quarter, no lower. */
        private const val LOW = 0.25f

        /** Vinyl's sound over this first share of a spin-up (and last of a spin-down). */
        private const val VINYL_SOUND = 0.35f
        private const val STEP_MS = 16L
        private const val GROUP_MARGIN_MS = 250L
        /** A clip not ready by then: the track restarts as ever. */
        private const val REWIND_WAIT_MS = 1_500L
        /** How often the position shown is told on while the clip sounds. */
        private const val REWIND_TICK_MS = 250L
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
