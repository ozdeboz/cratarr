/*
 * The announce channel (release 0.9, the owner, 2026-10-24): Home Assistant speaks through Cratarr's own player, so the
 * music ducks by the speech itself, as a radio does when the DJ talks. Android lets no app hear another's sound, so the
 * voice has to be played here, in this app's own audio.
 *
 * While music plays, the clip is mixed into it by [Mixer], the first step of the player's audio chain (SyncPlay): before
 * the tap that sends a leader's sound to its group, so a group hears it in step, and a player alone simply plays it. The
 * music's gain follows the clip's own loudness, a block at a time, looking a little ahead so it is down before the first
 * word, held through the gaps between words, and back up after the last.
 * While nothing plays, nothing runs through that chain: the clip then plays on a small player of its own, which takes
 * no audio focus (it would pause or duck the music by Android's own fixed step), after a short silence, so a Bluetooth
 * speaker that slept wakes before the first syllable.
 *
 * Each announcement is reported (PlayOnLink: `announce`): playing, then done once its last sample is heard (a group's
 * a buffer later), or failed with why. A second one while one plays waits its turn.
 */
package app.cratarr

import android.content.Context
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.ProgressiveMediaSource
import androidx.media3.exoplayer.source.SilenceMediaSource
import androidx.media3.datasource.DefaultDataSource
import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.ArrayDeque
import java.util.concurrent.Executors
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

@OptIn(UnstableApi::class)
object Announcer {
    private const val TAG = "CratarrAnnounce"

    /** The music under speech: about -14 dB. */
    internal const val DUCK = 0.2f
    /** A block of the clip whose loudness (RMS, 0..1) is above this counts as speech. */
    internal const val SPEECH = 0.02f
    /** The envelope's steps: how fast the music goes down, how long it stays down after a word, how slowly it returns. */
    internal const val ATTACK_MS = 30
    internal const val HOLD_MS = 220
    internal const val RELEASE_MS = 500
    /** How far ahead of the clip the music starts down, so it is down at the first sound. */
    internal const val LOOK_AHEAD_MS = 40
    /** The clip's loudness is measured in blocks this long. */
    internal const val BLOCK_MS = 10
    /** From the last sample mixed to its sound: the output's own latency, roughly (a Bluetooth speaker's included). */
    private const val HEARD_AFTER_MS = 250L
    /** Silence before a clip on a player that was idle, so a sleeping Bluetooth link wakes first. */
    private const val WAKE_MS = 400L
    /** How long a clip waits for the Ready service to come to the foreground before it plays alone. */
    private const val READY_WAIT_MS = 600L
    /** A mixed-in clip not out this long after its length (the music stopped under it) is given up. */
    private const val STUCK_AFTER_MS = 10_000L

    /** An announcement as reported, with its timeline (0.9.1) as far as it has come. */
    data class State(val id: String, val state: String, val error: String? = null, val timeline: Timeline? = null) {
        fun json(): JSONObject =
            JSONObject().put("id", id).put("state", state).apply {
                error?.let { put("error", it) }
                timeline?.let { put("timeline", it.json()) }
            }
    }

    /** What the mixer marks of a clip as it mixes it. */
    enum class Mark { STARTED, DUCKED, SPEECH_END, RELEASED }

    /**
     * An announcement's timeline (0.9.1, the owner: "better instrumentation"): the clip's length, and each moment in ms
     * since the app received the command: its first frame mixed (or played alone), the music first down at the duck,
     * the end of its last speech, the music back up, the announcement done; gave_up: given up (the music stopped under
     * it). The mix's moments are when the app mixed them; the sound is heard a buffer later.
     */
    class Timeline(private val received: Long) {
        var clip: Long? = null
        var started: Long? = null
        var ducked: Long? = null
        var speechEnd: Long? = null
        var released: Long? = null
        var done: Long? = null
        var gaveUp = false

        /** Ms since the command, now. */
        fun now(): Long = SystemClock.elapsedRealtime() - received

        fun json(): JSONObject = JSONObject().apply {
            clip?.let { put("clip_ms", it) }
            started?.let { put("started_ms", it) }
            ducked?.let { put("ducked_ms", it) }
            speechEnd?.let { put("speech_end_ms", it) }
            released?.let { put("released_ms", it) }
            done?.let { put("done_ms", it) }
            put("gave_up", gaveUp)
        }
    }

    private data class Request(
        val context: Context,
        val url: String,
        val id: String,
        val musicPlaying: () -> Boolean,
        val heardLater: () -> Long,
        val duck: (Boolean) -> Unit,
        val told: (State) -> Unit,
        val line: Timeline = Timeline(SystemClock.elapsedRealtime()),
    )

    private val main = Handler(Looper.getMainLooper())
    private val decoding = Executors.newSingleThreadExecutor()
    private val waiting = ArrayDeque<Request>()
    private var busy = false
    /** The mixer in the player's audio chain now (SyncPlay makes it). */
    @Volatile var mixer: Mixer? = null
    private var alone: ExoPlayer? = null

    /**
     * Speak the clip at `url`. `musicPlaying`: whether the player's music plays now (then it is mixed in);
     * `heardLater`: how much later than this player a group hears it (its buffer, while it leads; else 0); `duck`: the
     * player's own coarse duck, for a clip played beside music the mixer cannot take; `told`: its states. Main thread.
     */
    fun announce(
        context: Context,
        url: String,
        id: String,
        musicPlaying: () -> Boolean,
        heardLater: () -> Long,
        duck: (Boolean) -> Unit,
        told: (State) -> Unit,
    ) {
        waiting.add(Request(context.applicationContext, url, id, musicPlaying, heardLater, duck, told))
        next()
    }

    private fun next() {
        if (busy) return
        val r = waiting.poll() ?: return
        busy = true
        val m = mixer
        if (r.musicPlaying() && m != null && m.mixes) {
            // Decoded here, at the music's format, then mixed in as the music plays.
            val format = m.format
            decoding.execute {
                val clip = try {
                    decode(r.context, r.url, format.sampleRate, format.channelCount)
                } catch (e: Exception) {
                    Log.w(TAG, "cannot decode ${r.url}: ${e.message}")
                    null
                }
                main.post {
                    if (clip == null) return@post finish(r, State(r.id, "failed", "the clip could not be read", r.line))
                    r.line.clip = clip.frames * 1000L / clip.rate
                    r.told(State(r.id, "playing", timeline = r.line))
                    var over = false
                    // The music stopped under it (the chain no longer runs): given up a while after it should have ended.
                    val stuck = Runnable {
                        if (over) return@Runnable
                        over = true
                        m.stop()
                        r.line.gaveUp = true
                        r.line.done = r.line.now()
                        finish(r, State(r.id, "failed", "the music stopped under it", r.line))
                    }
                    main.postDelayed(stuck, clip.frames * 1000L / clip.rate + STUCK_AFTER_MS)
                    // The mixer's moments, in the clip's time from its first frame: since the command, from that frame.
                    val marks = { what: Mark, ms: Long ->
                        main.post {
                            when (what) {
                                Mark.STARTED -> r.line.started = r.line.now()
                                Mark.DUCKED -> r.line.ducked = (r.line.started ?: 0) + ms
                                Mark.SPEECH_END -> r.line.speechEnd = (r.line.started ?: 0) + ms
                                Mark.RELEASED -> r.line.released = (r.line.started ?: 0) + ms
                            }
                        }
                        Unit
                    }
                    m.play(clip, marks) {
                        main.post {
                            if (over) return@post
                            over = true
                            main.removeCallbacks(stuck)
                            main.postDelayed({
                                r.line.done = r.line.now()
                                finish(r, State(r.id, "done", timeline = r.line))
                            }, HEARD_AFTER_MS + r.heardLater())
                        }
                    }
                }
            }
        } else {
            playAlone(r)
        }
    }

    private fun finish(r: Request, state: State) {
        r.told(state)
        busy = false
        next()
    }

    /**
     * The clip on a player of its own: nothing plays (or the music's format is one the mixer does not take). From
     * Android 15 on, an app not on the screen starts no sound unless one of its services is in the foreground (its
     * request is ignored, Ready.kt): with music playing, the player's service is; else the Ready service is asked for a
     * moment (as a start of the music asks), and an announcement Android still will not let sound is reported failed,
     * with why, instead of done unheard.
     */
    private fun playAlone(r: Request) {
        if (r.musicPlaying() || Ready.foreground(r.context)) return soundAlone(r)
        Ready.ensure(r.context, "announce", briefly = true)
        main.postDelayed({
            if (Ready.foreground(r.context)) soundAlone(r)
            else finish(r, State(r.id, "failed", "Android lets the app make no sound from the background: turn on Stay ready", r.line.also { it.done = it.now() }))
        }, READY_WAIT_MS)
    }

    private fun soundAlone(r: Request) {
        val withMusic = r.musicPlaying()
        val p = alone ?: ExoPlayer.Builder(r.context)
            // No focus asked: a transient request would pause the music, or duck it by Android's own fixed step.
            .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_ASSISTANT).setContentType(C.AUDIO_CONTENT_TYPE_SPEECH).build(), false)
            .build()
            .also { alone = it }
        p.clearMediaItems()
        val clip = ProgressiveMediaSource.Factory(DefaultDataSource.Factory(r.context)).createMediaSource(MediaItem.fromUri(Uri.parse(r.url)))
        if (withMusic) p.setMediaSource(clip) else p.setMediaSources(listOf(SilenceMediaSource(WAKE_MS * 1000), clip))
        var said = false
        p.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (isPlaying && !said) {
                    said = true
                    r.line.started = r.line.now()
                    p.duration.takeIf { it > 0 }?.let { r.line.clip = it }
                    if (withMusic) {
                        r.duck(true)
                        r.line.ducked = r.line.now()
                    }
                    r.told(State(r.id, "playing", timeline = r.line))
                }
            }

            override fun onPlaybackStateChanged(state: Int) {
                if (state != Player.STATE_ENDED) return
                p.removeListener(this)
                if (withMusic) {
                    r.duck(false)
                    r.line.released = r.line.now()
                }
                r.line.done = r.line.now()
                finish(r, State(r.id, "done", timeline = r.line))
            }

            override fun onPlayerError(error: PlaybackException) {
                p.removeListener(this)
                if (withMusic) r.duck(false)
                r.line.done = r.line.now()
                finish(r, State(r.id, "failed", error.errorCodeName, r.line))
            }
        })
        p.prepare()
        p.play()
    }

    /** A clip as 16-bit PCM at `rate` and `channels`, its loudness a block at a time. */
    class Clip(val pcm: ShortArray, val channels: Int, val rate: Int, val loudness: FloatArray, val blockFrames: Int) {
        val frames get() = pcm.size / channels
    }

    /** The clip at `url` decoded (MediaExtractor and MediaCodec: what the device reads), resampled and remixed. */
    private fun decode(context: Context, url: String, rate: Int, channels: Int): Clip {
        val extractor = MediaExtractor()
        extractor.setDataSource(context, Uri.parse(url), null)
        val track = (0 until extractor.trackCount).first { extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true }
        extractor.selectTrack(track)
        val format = extractor.getTrackFormat(track)
        val codec = MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!)
        codec.configure(format, null, null, 0)
        codec.start()
        var inRate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        var inChannels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        val decoded = ArrayList<ShortArray>()
        val info = MediaCodec.BufferInfo()
        var inputDone = false
        var outputDone = false
        try {
            while (!outputDone) {
                if (!inputDone) {
                    val i = codec.dequeueInputBuffer(10_000)
                    if (i >= 0) {
                        val buf = codec.getInputBuffer(i)!!
                        val n = extractor.readSampleData(buf, 0)
                        if (n < 0) {
                            codec.queueInputBuffer(i, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            inputDone = true
                        } else {
                            codec.queueInputBuffer(i, 0, n, extractor.sampleTime, 0)
                            extractor.advance()
                        }
                    }
                }
                val o = codec.dequeueOutputBuffer(info, 10_000)
                when {
                    o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        val f = codec.outputFormat
                        inRate = f.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        inChannels = f.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                    }
                    o >= 0 -> {
                        val out = codec.getOutputBuffer(o)!!.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
                        val s = ShortArray(info.size / 2)
                        out.get(s)
                        decoded.add(s)
                        codec.releaseOutputBuffer(o, false)
                        if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outputDone = true
                    }
                }
            }
        } finally {
            codec.stop()
            codec.release()
            extractor.release()
        }
        val all = ShortArray(decoded.sumOf { it.size }).also { a -> var at = 0; decoded.forEach { it.copyInto(a, at); at += it.size } }
        val pcm = remix(all, inChannels, inRate, channels, rate)
        // Its loudness, a block at a time, the channels' mean.
        val block = max(1, rate * BLOCK_MS / 1000)
        val frames = pcm.size / channels
        val loudness = FloatArray((frames + block - 1) / block) { b ->
            var sum = 0.0
            val from = b * block
            val to = min(frames, from + block)
            for (f in from until to) {
                var v = 0f
                for (c in 0 until channels) v += pcm[f * channels + c] / 32768f
                v /= channels
                sum += v * v
            }
            sqrt(sum / max(1, to - from)).toFloat()
        }
        return Clip(pcm, channels, rate, loudness, block)
    }

    /** Interleaved 16-bit PCM from one rate and channel count to another (linear between samples; channels mixed down
     *  by their mean, or the first ones repeated up). */
    private fun remix(input: ShortArray, inChannels: Int, inRate: Int, channels: Int, rate: Int): ShortArray {
        val inFrames = input.size / inChannels
        val frames = (inFrames.toLong() * rate / inRate).toInt()
        val out = ShortArray(frames * channels)
        val mono = inChannels > channels
        for (f in 0 until frames) {
            val at = f.toDouble() * inRate / rate
            val i = at.toInt().coerceAtMost(inFrames - 1)
            val j = min(inFrames - 1, i + 1)
            val x = (at - i).toFloat()
            for (c in 0 until channels) {
                val v = if (mono) {
                    var a = 0f
                    var b = 0f
                    for (k in 0 until inChannels) {
                        a += input[i * inChannels + k]
                        b += input[j * inChannels + k]
                    }
                    (a + (b - a) * x) / inChannels
                } else {
                    val k = c % inChannels
                    input[i * inChannels + k] + (input[j * inChannels + k] - input[i * inChannels + k]) * x
                }
                out[f * channels + c] = v.toInt().coerceIn(-32768, 32767).toShort()
            }
        }
        return out
    }

    /**
     * The first step of the player's audio chain: the clip mixed into the music, the music ducked by the clip's own
     * loudness. Passes the music on untouched while no clip plays. Takes 16-bit PCM (the player's sink without float
     * output); in any other format it only passes the music on, and a clip plays on its own player.
     */
    class Mixer : BaseAudioProcessor() {
        @Volatile var format: AudioProcessor.AudioFormat = AudioProcessor.AudioFormat.NOT_SET
            private set
        val mixes: Boolean get() = format.encoding == C.ENCODING_PCM_16BIT && format.sampleRate > 0

        @Volatile private var clip: Clip? = null
        @Volatile private var ended: (() -> Unit)? = null
        /** Its moments as it mixes them (the announcement's timeline, 0.9.1): what, and when in the clip's time (ms
         *  from its first frame mixed). Called on the audio thread. */
        @Volatile private var marked: ((Mark, Long) -> Unit)? = null
        /** Where the clip is (frames), the music's gain now, and the last frame the clip was loud near. */
        private var at = 0
        private var gain = 1f
        private var loudUntil = -1
        /** For the marks: the music first down at the duck, back up after it, and where the last speech ended. */
        private var ducked = false
        private var released = false
        private var speechEnd = 0

        /** The clip given up: the music goes on as it was. */
        fun stop() {
            clip = null
            ended = null
            marked = null
            // The music comes back up at the release's pace (queueInput), not at once.
        }

        fun play(c: Clip, onMark: ((Mark, Long) -> Unit)? = null, onEnded: () -> Unit) {
            ended = onEnded
            marked = onMark
            at = 0
            loudUntil = -1
            ducked = false
            released = false
            speechEnd = 0
            clip = c
        }

        private fun mark(what: Mark, frame: Int, rate: Int) {
            marked?.invoke(what, frame * 1000L / rate)
        }

        override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat): AudioProcessor.AudioFormat {
            format = inputAudioFormat
            return inputAudioFormat
        }

        override fun queueInput(inputBuffer: ByteBuffer) {
            val n = inputBuffer.remaining()
            if (n == 0) return
            val out = replaceOutputBuffer(n)
            val c = clip
            // A clip given up while the music was down: the music rises again at the release's pace.
            if (c == null && mixes && gain < 0.999f) {
                val music = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
                val channels = format.channelCount
                val release = 1f / max(1, format.sampleRate * RELEASE_MS / 1000)
                val o = out.order(ByteOrder.LITTLE_ENDIAN)
                for (f in 0 until music.remaining() / channels) {
                    gain = min(1f, gain + release)
                    for (ch in 0 until channels) o.putShort((music.get() * gain).toInt().coerceIn(-32768, 32767).toShort())
                }
                inputBuffer.position(inputBuffer.limit())
                out.flip()
                return
            }
            if (c == null || !mixes || c.channels != format.channelCount || c.rate != format.sampleRate) {
                out.put(inputBuffer)
                out.flip()
                return
            }
            val music = inputBuffer.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
            val frames = music.remaining() / c.channels
            val rate = c.rate
            val attack = 1f / max(1, rate * ATTACK_MS / 1000)
            val release = 1f / max(1, rate * RELEASE_MS / 1000)
            val ahead = rate * LOOK_AHEAD_MS / 1000
            val hold = rate * HOLD_MS / 1000
            val o = out.order(ByteOrder.LITTLE_ENDIAN)
            for (f in 0 until frames) {
                if (at == 0) mark(Mark.STARTED, 0, rate)
                // Speech just ahead: the music stays down until a hold after it.
                val near = (at + ahead) / c.blockFrames
                if (near < c.loudness.size && c.loudness[near] > SPEECH) {
                    loudUntil = at + ahead + hold
                    speechEnd = min(c.frames, (near + 1) * c.blockFrames)
                }
                val target = if (at <= loudUntil) DUCK else 1f
                gain = if (gain > target) max(target, gain - attack) else min(target, gain + release)
                if (!ducked && gain <= DUCK + 0.001f) {
                    ducked = true
                    mark(Mark.DUCKED, at, rate)
                }
                if (ducked && !released && at > loudUntil && gain >= 0.999f) {
                    released = true
                    mark(Mark.SPEECH_END, speechEnd, rate)
                    mark(Mark.RELEASED, at, rate)
                }
                for (ch in 0 until c.channels) {
                    val m = music.get() * gain
                    val v = if (at < c.frames) c.pcm[at * c.channels + ch].toFloat() else 0f
                    o.putShort((m + v).toInt().coerceIn(-32768, 32767).toShort())
                }
                // On past the clip's end too: held at its end, the hold after its last words never passed and the music
                // stayed down until the give-up (the owner, 2026-10-24: "the duck exit was way too late").
                at++
            }
            inputBuffer.position(inputBuffer.limit())
            out.flip()
            // The clip is out and the music back up: done (its sound a little behind).
            if (at >= c.frames && gain >= 0.999f) {
                clip = null
                marked = null
                val e = ended
                ended = null
                e?.invoke()
            }
        }
    }
}
