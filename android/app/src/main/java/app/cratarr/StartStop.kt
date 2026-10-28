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
    /** The wind stream of the playing track (decision 0074): from a moment (ms), to one or to the edge (null), condensed
     *  to a length (ms) or open-ended (null), back or forward; none for a track Cratarr has no file of. */
    private val windUrl: (Long, Long?, Long?, Boolean, String?, Float, Float, Boolean) -> String?,
    /** The queue's tracks in a hold's way, nearest first (id, length in ms), where it winds on through the queue. */
    private val windWay: (Boolean) -> List<Pair<String, Long>>,
    /** Sync play: leading a group, speed and volume act on the sound sent on, and the tape's sound is the group's while
     *  it winds (the owner, 2026-10-27: "Sync groups don't wind together. Make it so."). */
    private val sync: SyncPlay,
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

    /**
     * Winding (decision 0074): the server's wind stream on a small player of its own (made the first time; it asks no
     * focus, the track's player holds it), its speed this device's envelope, the AudioTrack's rate with the pitch
     * following as Start and stop's vinyl: up from still over the spin-up, held, down over the brake, never above 1×. A
     * tap (Previous to the start, a seek where seeking winds) is condensed to its length, which the server fits to that
     * envelope; a hold winds until let go, or the edge. The track's player waits meanwhile, paused; the place shown
     * follows the wind.
     */
    private var tape: ExoPlayer? = null
    private var tapeListener: Player.Listener? = null
    private var tapeStarted = false
    /** What a wind that never sounded does instead (the plain jump); what follows one that did, from where it got. */
    private var tapeRestart: (() -> Unit)? = null
    private var tapeThen: ((Int, Long) -> Unit)? = null
    /** A tap that lands in the next track (Next's fast-forward): the jump there, made as it lands. */
    private var tapeJump: (() -> Unit)? = null
    /** The lengths of the tracks a hold winds on into (through the queue), nearest first. */
    private var tapeChain: List<Long> = emptyList()
    private var tapeHold = false
    private var tapeFrom = 0L
    private var tapeTo = 0L
    private var tapeSign = -1
    /** A tap's wall length; the speed limit; the spin-up and the brake (ms); the volume it plays at. */
    private var tapeMs = 1L
    private var tapeSpeed = 8f
    /** A hold's rise (0.9.12; the Tape page since 0.9.14): its speed doubles every so many seconds of its stream, up to
     *  its top. */
    private var tapeDoublingS = 1.5
    private var tapeTop = 64.0
    /** Armed while the tape's own stop runs, sounding as it ends ([tapeGo]); ready meanwhile; armed again from where
     *  the track stands if that is not where it was armed. */
    private var tapeWaits = false
    private var tapeReady = false
    private var tapeRearm: ((Long) -> Unit)? = null
    /** Next's fast-forward went on to the next track already (readied as its brake began). */
    private var tapeJumped = false
    /** The wind under way is Previous's rewind to the start: a second Previous goes to the track before. */
    private var tapePrevious = false
    private var tapeAttack = 400L
    private var tapeRelease = 250L
    private var tapeVolume = 1f
    /** When it began sounding, when a hold was let go and at what rate (elapsed realtime), and the rate now. */
    private var tapeSounded = 0L
    private var tapeReleased = 0L
    private var tapeReleaseFrom = 1f
    private var tapeRate = 0f
    /** A forward hold begun while playing (0.9.12, the owner): it does not stop first but flies off at the track's own
     *  speed, its stream at [FLY_SPEED] played at its lowest rate, and lets go at that rate back into the track. */
    private var tapeFlying = false
    /** The rate where the wind sounds at the track's own speed: a flying hold's [LOW] on its stream at [FLY_SPEED]; a
     *  flying wind to a point's 1 / its speed (0.9.14, the owner: a seek forward need not stop first), from its stream's
     *  length as it comes. */
    private var tapeFloor = LOW
    /** Its speed and volume on the sound itself ([Varispeed]) rather than Android's output: in a group, and for a
     *  flying wind to a point, whose floor goes below the quarter Android's output reaches. */
    private var tapeInChain = false
    /** The knob's backwards play under way (a wind at 1×, loud as the track, no rise), and what follows its brake:
     *  `forward` (the knob back at R: on with the wind-up) or `rest` (a pause: paused where it got). */
    private var tapeReverse = false
    /** A ramp of the tape's own (a stop before a wind, a knob turn, the wind-up after): shown as playing, no ramp told. */
    private var quietRamp = false
    private var reverseNext = "rest"
    /** A hold that could not wind: the plain press its letting go makes instead. */
    private var holdPlain: (() -> Unit)? = null

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

    /** A wind sounds now: the place shown follows it (0.9.10). */
    val rewinding: Boolean get() = tapeListener != null && tapeStarted

    /**
     * Asked to play from rest and not sounding yet (0.9.14, the owner: the waveform shows the truth, grey until it
     * sounds): from a play while still until the track first plays, or it is paused again. The tape's own stops and
     * starts are not this (the transport plays on): a wind's landing plays on as part of it.
     */
    private val startWatch = object : Player.Listener {
        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            val now = playWhenReady && !player.isPlaying && !unsaid && tapeListener == null && player.playbackState != Player.STATE_ENDED
            if (now != starting) {
                starting = now
                changed()
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (isPlaying && starting) {
                starting = false
                changed()
            }
        }
    }.also { player.addListener(it) }

    /** The place shown while it sounds, told on as it goes. */
    private val tapeTick = object : Runnable {
        override fun run() {
            val t = tape ?: return
            if (!rewinding) return
            heldPosition = reached()
            rewoundAt = heldPosition
            rewindListener?.invoke()
            // Its pace against the clock, once a second.
            if (tapeTicks++ % 4 == 0) step("clip pace", "clip_ms" to t.currentPosition, "buffered_ms" to t.bufferedPosition, "rate" to tapeRate)
            changed()
            main.postDelayed(this, REWIND_TICK_MS)
        }
    }

    /** The envelope on the wind's speed, each step: below the lowest speed it sounds at, it fades instead (as Start and
     *  stop's vinyl comes up); a hold's brake over, the wind is over. */
    private val tapeEnvelope = object : Runnable {
        override fun run() {
            val t = tape ?: return
            if (!rewinding) return
            val r = envelope(SystemClock.elapsedRealtime()) ?: return wound()
            tapeRate = r
            // Flying, never below the track's own speed and as loud as the track; from still, faded in below the lowest.
            if (tapeFlying) tapeAt(r, 1f) else tapeAt(max(r, LOW), min(1f, r / LOW))
            main.postDelayed(this, STEP_MS)
        }
    }

    /**
     * The wind's speed (the pitch following) and its share of the track's volume (0..1): on Android's output; leading a
     * group, on the sound itself, which goes on to the group before the output (silent here: the leader hears the group).
     */
    private fun tapeAt(speed: Float, share: Float) {
        val t = tape ?: return
        if (grouped() || tapeInChain) {
            sync.tapeSpeed.rate = speed
            sync.tapeSpeed.gain = share
            if (t.playbackParameters != PlaybackParameters.DEFAULT) t.playbackParameters = PlaybackParameters.DEFAULT
            val out = if (grouped()) 0f else tapeVolume
            if (t.volume != out) t.volume = out
        } else {
            sync.tapeSpeed.rate = 1f
            sync.tapeSpeed.gain = 1f
            t.playbackParameters = PlaybackParameters(speed, speed)
            t.volume = tapeVolume * share
        }
    }

    /** The track's speed, the pitch following: as [tapeAt], on the sound itself leading a group. */
    private fun trackAt(speed: Float) {
        if (grouped()) {
            sync.trackSpeed.rate = speed
            if (player.playbackParameters != PlaybackParameters.DEFAULT) player.playbackParameters = PlaybackParameters.DEFAULT
        } else {
            sync.trackSpeed.rate = 1f
            if (player.playbackParameters.speed != speed) player.playbackParameters = PlaybackParameters(speed, speed)
        }
    }

    /** A raised-cosine ramp's height [t] ms into [len] (0..1). */
    private fun eased(t: Long, len: Long): Float =
        if (len <= 0) 1f else ((1 - kotlin.math.cos(Math.PI * t.coerceIn(0L, len) / len)) / 2).toFloat()

    /** The envelope [now]: a tap's up from its start and down to its end (one too short for both peaks lower); a hold's
     *  up from its start and down from its letting go, from the rate it had then; null once a hold's brake is over. */
    private fun envelope(now: Long): Float? {
        val t = now - tapeSounded
        val up = eased(t, tapeAttack)
        // A flying one starts and ends on its floor: the track's own speed on its stream.
        val floor = if (tapeFlying) tapeFloor else 0f
        // A wind to a point lands as its envelope ends, whatever of its stream is left (2026-10-28, the tablet: Back to
        // the start sat at rate 0 by its clip's end until the player gave up, 9 s).
        if (!tapeHold) return if (t > tapeMs + TAP_GRACE_MS) null else floor + (1f - floor) * min(up, eased(tapeMs - t, tapeRelease))
        if (tapeReleased == 0L) return floor + (1f - floor) * up
        val left = tapeRelease - (now - tapeReleased)
        if (left <= 0) return null
        return floor + (tapeReleaseFrom - floor) * eased(left, tapeRelease)
    }

    /** A wind to a point's envelope over its length (ms), without a floor: what its stream would be from still. */
    private fun tapArea(): Double {
        var area = 0.0
        for (i in 0 until tapeMs) area += min(eased(i, tapeAttack), eased(tapeMs - i, tapeRelease))
        return area
    }

    /** How far a hold has gone (ms of track) by [played] ms of its stream: rising (a hold), or steady (the knob's). */
    private fun holdGone(played: Long): Long =
        if (tapeReverse) (played * tapeSpeed).toLong() else (accelGone(tapeSpeed.toDouble(), played / 1000.0) * 1000).toLong()

    /** The tape deck's direction knob (0.9.12; Play On's `direction`, the page's bridge op): L plays backwards at 1× on
     *  the wind stream, R forwards. Turned while playing, the tape brakes to a stop (the deck's one brake), then spins up
     *  the other way; paused, it only sets which way the next play goes. */
    fun setDirection(left: Boolean) {
        // Where nothing can be wound (a Bandcamp preview, the device's own music) the knob stays at R, the page told so.
        if (left && windUrl(player.currentPosition, null, null, true, null, 1f, 0f, false) == null) {
            note("direction", "to" to "L", "refused" to "cannot wind here")
            changed()
            return
        }
        val want = if (left) "L" else "R"
        note("direction", "to" to want, "from" to deckDirection, "playing" to player.isPlaying, "reverse" to tapeReverse)
        if (want == deckDirection) return
        deckDirection = want
        changed()
        if (left) {
            if (player.isPlaying && tapeListener == null && !busy) {
                // Armed from where the stop will stand while it brakes (2026-10-28: asked at the stop, 135–180 ms).
                if (playBack(player.currentPosition + brakeSpan(), waits = true)) {
                    tapeRearm = { at ->
                        stopTape()
                        if (!playBack(at)) play(true) { player.play() }
                    }
                    quietStop { tapeGo() }
                } else {
                    quietStop { play(true) { player.play() } }
                }
            }
        } else if (tapeReverse && tapeListener != null) {
            reverseNext = "forward"
            windStop()
        }
    }

    /**
     * A stop of the tape's own (0.9.12, the owner: "quiet stops"): before a hold winds from play, and a knob turned while
     * playing. Wound down as a pause is (the deck's one brake), but shown as playing throughout and told to no page; then
     * [then], the music paused where it was.
     */
    private fun quietStop(then: () -> Unit) {
        // The deck's one brake whatever Start and stop is set to (the owner: a hold never fades or cuts as a pause).
        val m = Prefs.START_STOP_VINYL
        if (!player.isPlaying) {
            // A group hears no break: the tape takes over from here (or the track again).
            sync.source.feed(null)
            player.pause()
            then()
            return
        }
        val at = player.currentPosition
        heldPosition = at
        quietRamp = true
        commit(true)
        // The tape's Brake (Start and stop's own stop is for a pause and a skip).
        val brake = prefs.tapeBrakeMs
        ramping("out", brake)
        val mine = token + 1
        ramp(1f, 0f, brake, setter(m), curve(m, down = true)) { whole ->
            if (!whole || token != mine) return@ramp
            sync.source.feed(null)
            player.pause()
            // Where the brake leaves it (the tape's own stop rests there; the wind was armed from there).
            // Full speed and volume for what comes next, the music paused meanwhile.
            running = null
            gain = 1f
            volume()
            Fader.target = 1f
            trackAt(1f)
            then()
        }
    }

    /**
     * The knob at L as a play starts (a wind landing, a play from rest): backwards from here; where it cannot (at the
     * start already, a track Cratarr has no file of), the knob back at R and told. True where it played backwards.
     */
    private fun knobBack(): Boolean {
        if (playBack()) return true
        note("direction", "to" to "R", "why" to "backwards could not start")
        deckDirection = "R"
        changed()
        return false
    }

    /** A wind's landing plays on ([start]), or backwards where the knob was turned to L meanwhile. */
    private fun landPlay(start: () -> Unit) {
        if (deckDirection == "L" && knobBack()) return
        start()
    }

    /** Back to forwards, the knob at R, any backwards play dropped (another track, a new queue, a seek). */
    fun forwards() {
        if (tapeReverse) {
            tapeRestart = null
            tapeThen = null
            stopTape()
            reset()
        }
        if (deckDirection != "L") return
        deckDirection = "R"
        changed()
    }

    /** Plays backwards from where it is (the knob at L): a wind at 1×, as loud as the track, spun up from still; false
     *  where it cannot (a track Cratarr has no file of, at the start already, in a group). */
    private fun playBack(from: Long = player.currentPosition, waits: Boolean = false): Boolean {
        if (tapeListener != null) return false
        if (from <= 0L) return false
        val way = if (prefs.windEdges == "queue") windWay(true) else emptyList()
        val url = windUrl(from, null, null, true, way.joinToString(",") { it.first }.ifEmpty { null }, 1f, 0f, false) ?: return false
        reverseNext = "rest"
        wind(hold = true, from = from, to = 0L, url = url, restart = null, chain = way.map { it.second }, speed = 1f, reverse = true, waits = waits) { k, at ->
            if (k == 0) player.seekTo(at) else player.seekTo((player.currentMediaItemIndex - k).coerceAtLeast(0), at)
            if (reverseNext == "forward") play(true) { player.play() }
        }
        return true
    }

    /** How far a hold has gone (seconds of track) by [secs] of its stream: speeding up from [base], doubling every
     *  [tapeDoublingS], up to [tapeTop] (the server's own formula, `wind::Accel`). */
    private fun accelGone(base: Double, secs: Double): Double {
        val top = tapeTop.coerceAtLeast(base)
        val doubling = tapeDoublingS
        val ts = (doubling * kotlin.math.log2(top / base)).coerceAtLeast(0.0)
        val up = { x: Double -> base * doubling / kotlin.math.ln(2.0) * (2.0.pow(x / doubling) - 1) }
        return if (secs <= ts) up(secs) else up(ts) + top * (secs - ts)
    }

    /** Where the wind has got the track to: a tap along its way as far as its stream has played (its cuts smoothed
     *  over), a hold at the speed it winds. */
    private fun reached(): Long {
        val t = tape ?: return tapeFrom
        val played = t.currentPosition.coerceAtLeast(0L)
        val whole = t.duration.takeIf { it > 0 }
        val gone = if (!tapeHold && whole != null) (kotlin.math.abs(tapeTo - tapeFrom) * played.toDouble() / whole).toLong() else holdGone(played)
        return (tapeFrom + tapeSign * gone).coerceIn(min(tapeFrom, tapeTo), max(tapeFrom, tapeTo))
    }

    /** Where a hold let go has got: which track (0 this one, then the ones it wound on into) and where in it. */
    private fun landing(): Pair<Int, Long> = landingAt((tape?.currentPosition ?: 0L).coerceAtLeast(0L))

    /** As [landing], [played] ms into its stream. */
    private fun landingAt(played: Long): Pair<Int, Long> {
        var left = holdGone(played)
        val first = kotlin.math.abs(tapeTo - tapeFrom)
        if (left <= first || tapeChain.isEmpty()) return 0 to (tapeFrom + tapeSign * min(left, first))
        left -= first
        tapeChain.forEachIndexed { i, d ->
            if (left <= d) return (i + 1) to (if (tapeSign < 0) d - left else left)
            left -= d
        }
        return tapeChain.size to (if (tapeSign < 0) 0L else tapeChain.last())
    }

    private val tapeLate = Runnable {
        if (tapeListener != null && !tapeStarted) {
            step("clip late")
            wound()
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
        if (!unsaid) changed()
    }

    /** While a wind lands: its state told once it is settled ([wound]). */
    private var unsaid = false

    /** A ramp's direction told to the page (the play button draws it): `out`, `in`, or none at its end. */
    private fun ramping(dir: String?, ms: Long, full: Long = ms) {
        running = dir
        // The tape's own stops and starts tell the page nothing: its play button moves for play and pause only.
        if (quietRamp) return
        // Whether it ends with the music stopped (a pause's, a stop's): the page greys its waveform and drains its play
        // button for those alone, not for a skip's wind-down or a wind-up.
        rampListener?.invoke(dir, ms, full, committed == false)
    }

    private fun note(what: String, vararg fields: Pair<String, Any?>) =
        log(what, arrayOf<Pair<String, Any?>>("mode" to prefs.startStopMode, "playing" to player.isPlaying, "grouped" to grouped(), *fields))

    private fun mode(): String {
        val m = prefs.startStopMode
        // Vinyl in a group too, its speed on the sound sent on ([trackAt]).
        return m
    }

    private fun ease(t: Float) = if (t < 0.5f) 2 * t * t else 1 - (-2 * t + 2) * (-2 * t + 2) / 2

    /** Steps [set] from [from] to [to] (0 still, 1 playing) over [ms], eased; then [done] (false: cut short). */
    /** A ramp's curve: Vinyl's speed eases in going down and out coming up (what the turntable does); Fade eases both. */
    private fun curve(mode: String, down: Boolean): (Float) -> Float = when {
        mode != Prefs.START_STOP_VINYL -> ::ease
        // The deck's one brake (0.9.12, the owner): a wind's own raised cosine.
        down -> { t -> ((1 - kotlin.math.cos(Math.PI * t)) / 2).toFloat() }
        else -> { t -> 1 - (1 - t) * (1 - t) }
    }

    /** A wind-down's length: Vinyl's is the deck's one brake (Tape winding's Brake, the owner); Fade's as set. */
    private fun downMs(@Suppress("UNUSED_PARAMETER") mode: String): Long = prefs.startStopStopMs

    /** A ramp's mean speed as a share of full: Vinyl's spin-up runs slower than the music, a fade at full speed. */
    private fun pace(mode: String): Float = if (mode == Prefs.START_STOP_VINYL) LOW + (1f - LOW) * 2f / 3f else 1f

    /** How long a wind-up from [at] (ms into the track) is: as set, but only as long as there is track before it to
     *  start early on (0.9.7, the owner: a 3 s fade from 1 s in is a 1 s one); none from its very start. */
    private fun windUp(mode: String, at: Long, full: Long = prefs.startStopMs): Long =
        if (at < FROM_START_MS) 0L else min(full, (at / pace(mode)).toLong())

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
            trackAt(LOW + (1f - LOW) * v)
            val loud = loudness(min(1f, v / VINYL_SOUND))
            if (grouped()) {
                Fader.target = loud
            } else {
                gain = loud
                volume()
            }
        }
    }

    /** Back to plain playing: full speed, its own pitch, full volume; a ramp under way stopped. */
    fun reset() {
        stopTape()
        quietRamp = false
        token++
        val next = after
        after = null
        next?.let { main.post(it) }
        heldPosition = null
        stopping = null
        level = 1f
        if (committed != null && running != null) ramping(null, 0)
        commit(null)
        Fader.target = 1f
        if (gain != 1f) {
            gain = 1f
            volume()
        }
        trackAt(1f)
    }

    /** Plays ([start]), winding up from still and silent. */
    fun play(start: () -> Unit) = play(false, start)

    /** As [play]; [quiet]: the tape's own wind-up after a wind, shown as playing throughout and told to no page. */
    fun play(quiet: Boolean, start: () -> Unit) {
        note("play", "busy" to busy, "direction" to deckDirection)
        // The knob at L: playing is backwards (0.9.12).
        // The knob at L: playing is backwards; where that cannot start, forwards, the knob too (the truth).
        if (deckDirection == "L" && tapeListener == null && !player.isPlaying && knobBack()) return
        if (busy) {
            // During a wind-down: played once it is over; during a wind-up, it plays already.
            if (committed == false) after = { play(start) }
            return
        }
        // The tape's own wind-up (after a wind, the knob back at R) is its spin-up, as a vinyl's; a play's as set.
        val m = if (quiet) Prefs.START_STOP_VINYL else mode()
        val full = if (quiet) prefs.tapeAttackMs else prefs.startStopMs
        val at = player.currentPosition
        // From a track's start: at once, always.
        val ms = windUp(m, at, full)
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
        quietRamp = quiet
        commit(true)
        ramping("in", ms, full)
        start()
        ramp(0f, 1f, ms, set, curve(m, down = false)) { whole -> if (whole) reset() }
    }

    /** Pauses or stops ([stop]), winding down first: once what was wound down has been heard. */
    fun pause(stop: () -> Unit) {
        note("pause", "busy" to busy)
        // Playing backwards: braked to rest where it gets, the knob still at L.
        if (tapeReverse && tapeListener != null) {
            reverseNext = "rest"
            windStop()
            return
        }
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
        ramping("out", downMs(m))
        val mine = token + 1
        ramp(1f, 0f, downMs(m), setter(m), curve(m, down = true)) { whole ->
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
        // During backwards play: there, forwards.
        if (tapeReverse) {
            forwards()
            go()
            player.play()
            return
        }
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
        // Another track plays forwards, the knob back at R.
        if (tapeReverse) {
            forwards()
            go()
            player.play()
            return
        }
        // During a wind-down (a skip's, a pause's brake) Next and Previous are not taken, from anywhere (the owner: one
        // tap, one skip).
        if (busy) return note("skip ignored", "ramp" to running)
        val m = mode()
        if (m == Prefs.START_STOP_OFF || !prefs.startStopSkips || !player.isPlaying || grouped()) {
            reset()
            go()
            return
        }
        heldPosition = player.currentPosition
        commit(true)
        ramping("out", downMs(m))
        ramp(1f, 0f, downMs(m), setter(m), curve(m, down = true)) { whole ->
            if (!whole) return@ramp
            // Full speed and volume first: the next one starts as it is.
            reset()
            go()
        }
    }

    /** A tap's length for winding [distance] ms of track (back where negative): its way's seconds per minute, 12 s at
     *  most. */
    private fun tapMs(distance: Long, motion: String): Long =
        (prefs.pointWind(motion).ratio * kotlin.math.abs(distance) / 60f).toLong().coerceIn(200L, WIND_CAP_MS)

    /**
     * Tape rewind: Previous that restarts the track winds it back to its start, then plays it from there as it is; false
     * where it does not (off, not playing, in a group, early in the track, where Previous goes to the one before, or a
     * track Cratarr has no file of). One not sounding within [REWIND_WAIT_MS], or failing, and the track restarts as
     * ever ([restart]).
     */
    fun rewind(restartAfterMs: Long, restart: () -> Unit): Boolean {
        // During backwards play Previous is a skip: forwards, and playing.
        if (tapeReverse) {
            forwards()
            restart()
            player.play()
            return true
        }
        note("rewind", "busy" to busy, "rewind" to prefs.startStopRewind, "at_ms" to player.currentPosition, "again" to tapePrevious)
        // A second Previous while the first one's rewind runs (the owner: a double tap): the track before, the tape
        // braking first where Start and stop is asked for next and previous too, else at once.
        if (tapePrevious && tapeListener != null) {
            val before = {
                landed = null
                restart()
                player.play()
            }
            if (!tapeStarted || mode() == Prefs.START_STOP_OFF || !prefs.startStopSkips) {
                stopTape()
                reset()
                before()
            } else {
                tapeThen = { _, _ -> before() }
                tapeHold = true
                tapeReleased = SystemClock.elapsedRealtime()
                tapeReleaseFrom = tapeRate
            }
            return true
        }
        if (busy) return true
        // Back to the start off: Previous goes there plainly.
        if (!prefs.windsBy("toStart") || !player.isPlaying) return false
        val from = player.currentPosition
        if (from <= restartAfterMs) return false
        val w = prefs.pointWind("toStart")
        val url = windUrl(from, 0L, tapMs(-from, "toStart"), true, null, w.speed, w.gainDb, false) ?: return false
        wind(hold = false, from = from, to = 0L, url = url, restart = restart, motion = "toStart") { _, _ -> landPlay { player.play() } }
        tapePrevious = true
        return true
    }

    /**
     * Next as a tape's fast-forward (0.9.14, the mirror of [rewind]): wound on to the track's end, condensed as a tap,
     * then the next track ([jump]) from its start; false where it does not (off, not playing, playing backwards, near the
     * end already, at the queue's end, a track Cratarr has no file of): the caller skips as ever.
     */
    fun fastForward(jump: () -> Unit): Boolean {
        if (tapeReverse) return false
        note("fast forward", "busy" to busy, "on" to prefs.startStopRewind, "at_ms" to player.currentPosition)
        // During a wind-down: not taken (as [skip]).
        if (busy) return true
        // On to the end off: Next skips plainly (Start and stop's skip wind-down as set).
        if (!prefs.windsBy("toEnd") || !player.isPlaying || !player.hasNextMediaItem()) return false
        val end = player.duration.takeIf { it > 0 } ?: return false
        // It flies (0.9.14): from a moment ahead, where the track will be when its stream sounds, never stopping first.
        val from = player.currentPosition + FLY_AHEAD_MS
        if (end - from <= SEEK_WIND_LEAST_MS) return false
        val w = prefs.pointWind("toEnd")
        val url = windUrl(from, end, tapMs(end - from, "toEnd"), false, null, w.speed, w.gainDb, false)?.let { "$it&fly=1" } ?: return false
        // Never sounding: the plain jump, still playing.
        wind(hold = false, from = from, to = end, url = url, restart = jump, flying = true, jump = jump, motion = "toEnd") { _, _ ->
            jump()
            landPlay { player.play() }
        }
        return true
    }

    /**
     * A seek where seeking winds the tape (decision 0074): wound there, condensed, then on from there as it is; a jump as
     * ever otherwise (off, not playing, in a group, a short way, a ramp under way, a track Cratarr has no file of).
     */
    fun seekTo(target: Long, wind: Boolean = false, go: () -> Unit) {
        val from = player.currentPosition
        // Each seek's way, to follow a device's report (the owner: "seek to wind does not yet work").
        // A seek back or forward by where it goes: its own motion.
        val motion = if (target < from) "seekBack" else "seekFwd"
        note("seek", "to_ms" to target, "from_ms" to from, "winds" to prefs.windsBy(motion), "asked" to wind, "busy" to busy, "playing" to player.isPlaying)
        // `wind`: a page driving this player asks it (its device's own motion on), whatever this device's setting.
        if ((prefs.windsBy(motion) || wind) && !busy && player.isPlaying && kotlin.math.abs(target - from) > SEEK_WIND_LEAST_MS) {
            // Forward it flies (0.9.14, the owner): from where the track will be when its stream sounds, taking over and
            // handing back at the track's own speed. Back it stops first (the way turns).
            val fly = target > from + FLY_AHEAD_MS + SEEK_WIND_LEAST_MS
            val start = if (fly) from + FLY_AHEAD_MS else from
            val w = prefs.pointWind(motion)
            val url = windUrl(start, target, tapMs(target - start, motion), target < from, null, w.speed, w.gainDb, false)?.let { if (fly) "$it&fly=1" else it }
            if (url != null) {
                wind(hold = false, from = start, to = target, url = url, restart = go, flying = fly, motion = motion) { _, _ -> landPlay { player.play() } }
                return
            }
        }
        seek(go)
    }

    /**
     * Previous or Next held (decision 0074, Play On's `wind` start): paused as a pause here would be, then wound back or
     * forward as long as held, to the edge at the latest; let go ([windStop]), braked, then on from where it got with
     * the wind-up. False where it cannot wind (in a group, a track Cratarr has no file of): the caller skips as ever.
     */
    fun windStart(back: Boolean, plain: () -> Unit): Boolean {
        note("wind", "phase" to "start", "back" to back, "busy" to busy, "on" to prefs.startStopRewind)
        holdPlain = null
        if (busy || tapeListener != null) return busy
        // Winding off here, in a group, a track Cratarr has no file of: nothing now, a plain press on letting go (a page
        // driving this player offers the hold whatever this device says).
        // Holding that way off: the press stays a tap (a plain one on letting go), the held Next's speed-up too.
        if (!prefs.windsBy(if (back) "holdBack" else "holdFwd")) return false.also { holdPlain = plain }
        // Forward while playing: flies off (no stop first) from a moment ahead, where the track will be when it sounds.
        val flying = !back && player.isPlaying
        val from = player.currentPosition + if (flying) FLY_AHEAD_MS else 0L
        val edge = if (back) 0L else player.duration.takeIf { it > 0 } ?: return false.also { holdPlain = plain }
        if (from == edge) return false.also { holdPlain = plain }
        // Through the queue where the device says so: on into the tracks in the way, landing in whichever it reached.
        val way = if (prefs.windEdges == "queue") windWay(back) else emptyList()
        val hold = prefs.holdWind(back)
        val speed = if (flying) FLY_SPEED else hold.speed
        val url = windUrl(from, null, null, back, way.joinToString(",") { it.first }.ifEmpty { null }, speed, hold.gainDb, true)
        if (url == null) {
            // A track that cannot wind (a Bandcamp preview, the device's own music; the owner, 2026-10-27: "gracefully
            // degrading, keeping whatever we can"): a held Next while playing speeds the track itself up to [FAST]×, the
            // pitch following, and lets it back down on letting go; a held Previous is a plain one on letting go.
            if (!back && player.isPlaying) return true.also { fastStart() }
            return false.also { holdPlain = plain }
        }
        val through = way.joinToString(",") { it.first }.ifEmpty { null }
        val go = { at: Long, waits: Boolean ->
            val u = if (at == from) url else windUrl(at, null, null, back, through, speed, hold.gainDb, true)
            if (u != null) {
                wind(hold = true, from = at, to = edge, url = u, restart = { play { player.play() } }, chain = way.map { it.second }, flying = flying, speed = speed, waits = waits) { k, land ->
                    if (k == 0) player.seekTo(land) else player.seekTo((player.currentMediaItemIndex + if (back) -k else k).coerceIn(0, player.mediaItemCount - 1), land)
                    // A flying one hands back while still moving; one from still winds up, quietly (the tape's own).
                    if (flying) landPlay { player.play() } else play(true) { player.play() }
                }
            }
        }
        // Back from play: the tape's own quiet stop first (shown as playing), the wind armed meanwhile from where the
        // stop will stand (2026-10-28: asked at the stop's end, ~200 ms); from a pause at once.
        if (player.isPlaying && !flying) {
            go(player.currentPosition + brakeSpan(), true)
            tapeRearm = { at ->
                stopTape()
                go(at, false)
            }
            quietStop { tapeGo() }
        } else {
            go(from, false)
        }
        return true
    }

    /** A held Next on a track that cannot wind: its own speed up to [FAST]× over the spin-up, eased, the pitch following;
     *  let go ([windStop]), back down to 1× over the brake. */
    private var fastAt = 0L
    private var fastReleased = 0L
    private var fastFrom = 1f
    private var fastSpeed = 1f
    private val fastStep = object : Runnable {
        override fun run() {
            if (fastAt == 0L) return
            val now = SystemClock.elapsedRealtime()
            fastSpeed = if (fastReleased == 0L) {
                1f + (FAST - 1f) * eased(now - fastAt, prefs.tapeAttackMs)
            } else {
                val release = prefs.tapeBrakeMs
                val left = release - (now - fastReleased)
                if (left <= 0) {
                    fastAt = 0L
                    trackAt(1f)
                    return
                }
                1f + (fastFrom - 1f) * eased(left, release)
            }
            trackAt(fastSpeed)
            main.postDelayed(this, STEP_MS)
        }
    }

    private fun fastStart() {
        note("wind", "phase" to "fast", "speed" to FAST)
        fastAt = SystemClock.elapsedRealtime()
        fastReleased = 0L
        fastSpeed = 1f
        main.removeCallbacks(fastStep)
        main.post(fastStep)
    }

    /** A hold let go: the brake from the rate it has got to; a wind not sounding yet is dropped, the track playing on. */
    fun windStop() {
        if (fastAt != 0L && fastReleased == 0L) {
            note("wind", "phase" to "stop", "fast" to fastSpeed)
            fastReleased = SystemClock.elapsedRealtime()
            fastFrom = fastSpeed
            return
        }
        note("wind", "phase" to "stop", "winding" to (tapeListener != null), "started" to tapeStarted, "plain" to (holdPlain != null))
        holdPlain?.let {
            holdPlain = null
            it()
            return
        }
        if (!tapeHold || tapeListener == null) return
        if (!tapeStarted) {
            val back = tapeReverse
            tapeRestart = null
            stopTape()
            reset()
            // The knob's backwards play paused before it sounded rests; a hold let go plays on.
            if (!back) play { player.play() }
            return
        }
        if (tapeReleased != 0L) return
        tapeReleased = SystemClock.elapsedRealtime()
        tapeReleaseFrom = tapeRate
        prepareLanding()
    }

    /** A wind begun: its stream prepared on the tape's player; once it can sound, the track waits, paused (a tap's at
     *  where it lands, buffered), and the wind plays under its envelope. */
    private fun wind(
        hold: Boolean,
        from: Long,
        to: Long,
        url: String,
        restart: (() -> Unit)?,
        chain: List<Long> = emptyList(),
        flying: Boolean = false,
        speed: Float = Prefs.POINT_SPEED,
        /** A wind to a point's motion (its duration); none for a hold. */
        motion: String = if (to < from) "toStart" else "toEnd",
        reverse: Boolean = false,
        /** A tap that lands in the next track (Next's fast-forward): the track goes there as it lands. */
        jump: (() -> Unit)? = null,
        /** Armed while the tape's own stop runs: it sounds as the stop ends ([tapeGo]). */
        waits: Boolean = false,
        then: (Int, Long) -> Unit,
    ) {
        tapeT0 = SystemClock.elapsedRealtime()
        tapeTicks = 0
        step("pressed", "hold" to hold, "from_ms" to from, "to_ms" to to, "url" to url.substringAfter("/api/stream/"))
        watchTrack()
        tapeHold = hold
        tapeFlying = flying
        // Until its stream says otherwise: a hold's lowest rate, a wind to a point's 1 / the speed asked.
        tapeFloor = if (flying && !hold) 1f / speed.coerceAtLeast(1f) else LOW
        tapeInChain = flying && !hold
        tapeReverse = reverse
        tapeChain = chain
        tapeFrom = from
        tapeTo = to
        tapeSign = if (to < from) -1 else 1
        tapeMs = if (hold) 0L else tapMs(to - from, motion)
        tapeSpeed = speed.coerceIn(1f, 16f)
        // The tape's spin-up and its one brake; a hold's way of getting faster.
        val way = prefs.holdWind(to < from)
        tapeAttack = way.attackMs
        tapeRelease = way.releaseMs
        tapeDoublingS = way.doublingS.toDouble()
        // A hold's way follows its flight (on to [Prefs.FLIGHT_TOP]); the knob's backwards play does not rise.
        tapeTop = if (hold && !reverse) Prefs.FLIGHT_TOP.toDouble() else way.top.toDouble()
        tapeJump = jump
        tapeJumped = false
        tapeWaits = waits
        tapeReady = false
        tapeVolume = player.volume
        tapeSounded = 0L
        tapeReleased = 0L
        tapeReleaseFrom = 1f
        tapeRate = 0f
        heldPosition = from
        // A tap and a flying hold show playing throughout; a hold from still shows the pause it began with (the play
        // button's fill moves for play and pause only, the owner).
        // Every wind shows playing (the tape's own stops are quiet, the owner).
        commit(true)
        val t = tape ?: ExoPlayer.Builder(context, sync.tapeRenderers(context))
            .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(), false)
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(context, Downloads.http(context))))
            .build()
            .also { tape = it }
        val listener = object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (tapeListener !== this) return
                step("clip ${stateName(state)}", "clip_ms" to t.currentPosition, "length_ms" to t.duration.takeIf { it > 0 })
                if (state == Player.STATE_READY && !tapeStarted && tapeWaits) {
                    // Armed while the tape's own stop runs: it sounds as the stop ends ([tapeGo]).
                    tapeReady = true
                    step("clip armed")
                } else if (state == Player.STATE_READY && !tapeStarted) {
                    sound()
                } else if (state == Player.STATE_ENDED) {
                    // A tap's stream played out; a hold at the edge.
                    wound()
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (tapeListener !== this) return
                step(if (isPlaying) "clip playing" else "clip not playing", "clip_ms" to t.currentPosition)
            }

            override fun onPlayerError(error: PlaybackException) {
                if (tapeListener !== this) return
                step("clip failed", "error" to error.errorCodeName, "why" to error.message)
                wound()
            }
        }
        tapeListener = listener
        tapeStarted = false
        tapeRestart = restart
        tapeThen = then
        t.addListener(listener)
        // As it starts already: what it plays before it sounds goes to a group as it will be heard.
        tapeAt(if (flying) tapeFloor else LOW, if (flying) 1f else 0f)
        t.setMediaItem(MediaItem.fromUri(url))
        t.playWhenReady = false
        t.prepare()
        step("clip prepared", "waits" to waits)
        // One armed during a stop: its wait counts from the stop's end.
        if (!waits) main.postDelayed(tapeLate, REWIND_WAIT_MS)
    }

    /** A wind over (played out, a hold braked, failed or too late): what follows it from where it got (a hold's place, a
     *  tap's end), or the plain jump where it never sounded. */
    private fun wound() {
        val restart = tapeRestart
        val then = tapeThen
        // Next's fast-forward lands at the next track's start.
        val (track, place) = if (tapeHold) landing() else 0 to if (tapeJump != null) 0L else tapeTo
        val jumped = tapeJumped
        // A group hears the track come back after the wind, no break between (the track takes over as it plays). One
        // that never sounded hands nothing over: the track may still be playing, its jump a plain one.
        if (tapeStarted) sync.source.feed(null)
        val started = stopTape()
        step(if (started) "track on from where the wind got" else "plain jump", "track" to track, "at_ms" to place, "track_state" to stateName(player.playbackState))
        // Told once, as it goes on from here (playing on, or resting): never a moment paused between (the owner: the
        // play button flashed at a hold's ends).
        unsaid = true
        reset()
        // No wind counts as listening (0074): where it landed is not heard (the player's count takes it from here).
        if (started) landed = place
        if (started && jumped) player.play() else if (started) then?.invoke(track, place) else restart?.invoke()
        unsaid = false
        changed()
    }

    /** The wind sounds: the track waits, paused (a tap's where it lands, buffered), the tape plays under its envelope. */
    private fun sound() {
        val t = tape ?: return
        tapeStarted = true
        main.removeCallbacks(tapeLate)
        // A group hears the wind from here, on from the track's sound, no break.
        sync.source.feed(SyncPlay.Source.Feed.TAPE)
        player.pause()
        // A tap waits where it lands, buffered; Next's fast-forward goes on to the next track as it lands.
        if (!tapeHold && tapeJump == null) player.seekTo(tapeTo)
        tapeSounded = SystemClock.elapsedRealtime()
        // A flying wind to a point: its floor from its stream's length, which the server fitted to the envelope
        // resting on it (D·f + (1 − f)·area). One fitted from still (a server before 0.9.14) does not fly.
        if (tapeFlying && !tapeHold) {
            val whole = t.duration
            val area = tapArea()
            val f = if (whole > 0 && tapeMs > area) ((whole - area) / (tapeMs - area)).toFloat() else 0f
            if (f >= FLY_FLOOR_LEAST) tapeFloor = min(1f, f) else tapeFlying = false
            step("fly", "floor" to tapeFloor, "flying" to tapeFlying, "length_ms" to whole)
        }
        // A flying one sounds at once, as loud as the track, on its floor; one from still comes up out of
        // silence.
        tapeAt(if (tapeFlying) tapeFloor else LOW, if (tapeFlying) 1f else 0f)
        t.play()
        main.post(tapeEnvelope)
        main.post(tapeTick)
        // Next's fast-forward: the next track readied as its brake begins, so its landing finds it loaded.
        if (tapeJump != null && !tapeHold) main.postDelayed(tapeNextReady, (tapeMs - tapeRelease).coerceAtLeast(0L))
    }

    /** The tape's own stop over: an armed wind sounds now if it starts near where the track stands ([ARM_NEAR_MS]),
     *  else it is armed again from there (handover parity with the page, 2026-10-28). */
    private fun tapeGo() {
        if (tapeListener == null) return
        tapeWaits = false
        val stand = player.currentPosition
        if (kotlin.math.abs(stand - tapeFrom) > ARM_NEAR_MS) {
            step("armed again", "stand_ms" to stand, "armed_ms" to tapeFrom)
            tapeRearm?.invoke(stand)
            return
        }
        main.postDelayed(tapeLate, REWIND_WAIT_MS)
        if (tapeReady) sound()
    }

    /** How far (ms of track) the track goes on while the tape's own stop brakes it from full speed: the Brake's length
     *  times its mean speed, measured from its curve (the page's rampSpan). */
    private fun brakeSpan(): Long {
        val c = curve(Prefs.START_STOP_VINYL, down = true)
        val n = 64
        val mean = (0 until n).sumOf { c((it + 0.5f) / n).toDouble() } / n
        return (prefs.tapeBrakeMs * (LOW + (1 - LOW) * (1 - mean))).toLong()
    }

    /**
     * Where a hold let go will land, the stream its brake plays on (its rate over the brake) added to where it has got:
     * the track readied there now, from [PREP_LEAD_MS] before it, so the landing finds it loaded (2026-10-28: it was
     * sought only at the landing, ~175 ms).
     */
    private fun prepareLanding() {
        val t = tape ?: return
        val n = 64
        var sum = 0.0
        for (i in 0 until n) {
            val e = (1 + kotlin.math.cos(Math.PI * (i + 0.5) / n)) / 2
            val r = tapeFloorOf() + (tapeReleaseFrom - tapeFloorOf()) * e
            sum += if (tapeFlying) r else max(r.toFloat(), LOW).toDouble()
        }
        val played = t.currentPosition.coerceAtLeast(0L) + (tapeRelease * sum / n).toLong()
        val (k, place) = landingAt(played)
        val index = (player.currentMediaItemIndex + tapeSign * k).coerceIn(0, (player.mediaItemCount - 1).coerceAtLeast(0))
        player.seekTo(index, (place - PREP_LEAD_MS).coerceAtLeast(0L))
        step("landing readied", "track" to k, "at_ms" to place)
    }

    /** A hold's lowest rate under its envelope: a flying one's floor, else still. */
    private fun tapeFloorOf(): Float = if (tapeFlying) tapeFloor else 0f

    /** Next's fast-forward: on to the next track as its brake begins (paused, loading). */
    private val tapeNextReady = Runnable {
        if (tapeListener != null && tapeStarted && !tapeJumped) {
            tapeJump?.invoke()
            tapeJumped = true
            step("next readied")
        }
    }

    /** Stops the wind, if one plays or is coming; whether it had sounded. */
    private fun stopTape(): Boolean {
        val l = tapeListener ?: return false
        tapeListener = null
        tapeRestart = null
        tapeThen = null
        tapeJump = null
        tapePrevious = false
        tapeWaits = false
        tapeReady = false
        tapeRearm = null
        main.removeCallbacks(tapeNextReady)
        main.removeCallbacks(tapeLate)
        main.removeCallbacks(tapeTick)
        main.removeCallbacks(tapeEnvelope)
        if (rewoundAt != null) {
            rewoundAt = null
            rewindListener?.invoke()
        }
        tape?.let {
            it.removeListener(l)
            it.stop()
            it.clearMediaItems()
            it.playbackParameters = PlaybackParameters.DEFAULT
        }
        sync.tapeSpeed.rate = 1f
        sync.tapeSpeed.gain = 1f
        tapeReverse = false
        return tapeStarted.also { tapeStarted = false }
    }

    /** The wind's player let go (the service ends). */
    fun release() {
        stopTape()
        tape?.release()
        tape = null
    }

    companion object {
        /** Told when a ramp starts (`out`, `in`, with its length, the length set, and whether it ends stopped) and ends
         *  (none): the page's player draws it. */
        @Volatile
        var rampListener: ((String?, Long, Long, Boolean) -> Unit)? = null

        /** Where Tape rewind's way back is now (ms into the track) while its clip sounds; null otherwise. The app's page
         *  shows it (`PagePlayer`), told each step by [rewindListener] and once more when it ends. */
        @Volatile
        var rewoundAt: Long? = null
            private set

        @Volatile
        var rewindListener: (() -> Unit)? = null

        /** Asked to play from rest and not sounding yet (0.9.14; [startWatch]): reported to Cratarr and the page. */
        @Volatile
        var starting = false
            private set

        /** The tape deck's direction knob as set (0.9.12): `L` backwards, `R` forwards; reported to Cratarr and the page. */
        @Volatile
        var deckDirection = "R"
            private set

        /** Where the last wind landed (ms into the track it landed in), until the player's count has taken it: what the
         *  wind passed over is not heard (decision 0074). */
        @Volatile
        var landed: Long? = null

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
        /** A tap's and a winding seek's longest (a hold has no end but the edge). */
        private const val WIND_CAP_MS = 12_000L
        /** A flying forward hold's stream speed: at the lowest rate ([LOW]) it sounds at the track's own speed. */
        private const val FLY_SPEED = 1f / LOW
        /** It starts this far ahead of where it was pressed: where the track will be when its stream sounds. */
        private const val FLY_AHEAD_MS = 500L
        /** An armed wind is taken only if it begins this near where the track stands; the track readied this far before
         *  a predicted landing (the page's, 2026-10-28). */
        private const val ARM_NEAR_MS = 100L
        private const val PREP_LEAD_MS = 500L
        /** A flying wind to a point's floor below this: its stream was fitted from still (an older server). */
        private const val FLY_FLOOR_LEAST = 0.03f
        /** A held Next on a track that cannot wind speeds it up to this (the page's own, 0.9.13). */
        private const val FAST = 4f
        /** A wind to a point lands this long after its envelope ends at the latest (its stream's last bit let go). */
        private const val TAP_GRACE_MS = 250L
        /** A seek nearer than this jumps, winding or not. */
        private const val SEEK_WIND_LEAST_MS = 2_000L
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
