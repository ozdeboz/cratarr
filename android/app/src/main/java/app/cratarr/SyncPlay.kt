/*
 * Sync play (decision 0037, as amended): this device in a group. Cratarr says the group it plays in, by its leader.
 *
 * - Leading, this app plays the group's queue as it plays alone, its own sound silent, and sends what it plays to
 *   Cratarr (Source), which stamps it and sends it on to every device of the group on Snapcast's protocol.
 * - Every device of a group, the leader too, plays that sound with Snapcast's own player (Snapclient), each chunk at its
 *   time: the group sounds as one.
 * - Following, it plays nothing of its own (decision 0037: it keeps no queue while grouped). Playing something here
 *   again takes it out of the group.
 */
package app.cratarr

import android.content.Context
import android.media.AudioManager
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import org.json.JSONObject
import java.io.BufferedOutputStream
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.math.roundToInt

@OptIn(UnstableApi::class)
class SyncPlay(private val context: Context, private val prefs: Prefs) : Player.Listener {
    private val main = Handler(Looper.getMainLooper())
    val source = Source()
    val silencer = Silencer()
    private val snapclient = Snapclient(context)
    private var player: Player? = null

    /** The group's leader; null while it plays alone. */
    @Volatile var leader: String? = null
        private set

    /** From a chunk's stamp to its sound: how far this player, leading, runs ahead of what is heard. */
    @Volatile var bufferMs = 1000L
        private set

    val leads: Boolean get() = leader != null && leader == prefs.playerName
    val follows: Boolean get() = leader != null && leader != prefs.playerName

    /** Told when it starts or stops following (its notification). */
    var onChange: () -> Unit = {}

    fun attach(player: Player) {
        this.player = player
        player.addListener(this)
    }

    /** The audio path of this app's player: its sound tapped for Cratarr and silenced here while it leads. */
    fun renderers(context: Context) = object : DefaultRenderersFactory(context) {
        override fun buildAudioSink(context: Context, enableFloatOutput: Boolean, enableAudioTrackPlaybackParams: Boolean): AudioSink {
            val sink = DefaultAudioSink.Builder(context)
                .setEnableFloatOutput(enableFloatOutput)
                .setEnableAudioTrackPlaybackParams(enableAudioTrackPlaybackParams)
                .setAudioProcessors(arrayOf<AudioProcessor>(TeeAudioProcessor(source), silencer))
                .build()
            // A seek, a jump or a stop: the sound breaks off here, in order with what was played before it.
            return object : ForwardingAudioSink(sink) {
                override fun flush() {
                    source.brk()
                    super.flush()
                }

                override fun reset() {
                    source.brk()
                    super.reset()
                }
            }
        }
    }

    /** Cratarr's word (main thread): the group it plays in, where its sound is (on `host`, else on the host Cratarr's
     *  pages come from). */
    fun group(leader: String?, host: String?, port: Int, bufferMs: Long, secret: String) {
        val host = host ?: prefs.serverUrl?.let { Uri.parse(it.trim()).host }
        // What Cratarr's sync server knows this player by (decision 0051): its secret, a colon, its name.
        val me = "$secret:${prefs.playerName}"
        val wasLeading = leads
        val wasFollowing = follows
        this.leader = leader?.takeIf { host != null }
        this.bufferMs = bufferMs.coerceIn(0, 10_000)
        val p = player
        when {
            this.leader == null -> {
                snapclient.stop()
                source.stop()
                silencer.silent = false
                // Its own speaker again, from where the group was heard.
                if (wasLeading && p != null) back(p)
            }
            leads -> {
                silencer.silent = true
                source.start(host!!, port, me)
                snapclient.start(host, port, me)
            }
            else -> {
                source.stop()
                silencer.silent = false
                p?.stop()
                p?.clearMediaItems()
                snapclient.start(host!!, port, me)
            }
        }
        if (wasFollowing != follows) onChange()
        Log.i(TAG, "group: ${this.leader ?: "none"}")
    }

    /** What is heard runs a buffer behind this player while it leads: its place as heard. */
    fun heardPosition(p: Player): Long =
        if (leads && p.isPlaying) (p.currentPosition - bufferMs).coerceAtLeast(0) else p.currentPosition

    private fun back(p: Player) {
        if (p.mediaItemCount == 0 || p.playbackState == Player.STATE_ENDED) return
        p.seekTo(p.currentMediaItemIndex, (p.currentPosition - bufferMs).coerceAtLeast(0))
    }

    override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
        val p = player ?: return
        // Paused while leading: back to where the group was heard, so it goes on from there (the seek also breaks the
        // sound off, so the group falls silent at once).
        if (!playWhenReady && leads && reason != Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) back(p)
    }

    override fun onIsPlayingChanged(isPlaying: Boolean) {
        // Played here while following: it plays alone again.
        if (isPlaying && follows) leave()
    }

    /** Out of the group, at Cratarr too. */
    fun leave() {
        val server = prefs.serverUrl?.trimEnd('/') ?: return
        val url = "$server/api/players/${Uri.encode(prefs.playerName)}/command"
        group(null, null, 0, bufferMs, "")
        Thread {
            try {
                val c = Net.open(context, url)
                c.requestMethod = "POST"
                c.connectTimeout = 5_000
                c.readTimeout = 5_000
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(JSONObject().put("command", "leave").toString().toByteArray()) }
                Net.code(context, c)
                c.disconnect()
            } catch (e: IOException) {
                Log.w(TAG, "leave: ${e.message}")
            }
        }.start()
    }

    fun release() {
        player?.removeListener(this)
        snapclient.stop()
        source.stop()
        main.removeCallbacksAndMessages(null)
    }

    /** Silent while it leads (the group's sound comes through Snapclient); its player still plays, at its pace. */
    class Silencer : BaseAudioProcessor() {
        @Volatile var silent = false
        private var zeros = ByteArray(0)

        override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat) = inputAudioFormat

        override fun queueInput(inputBuffer: ByteBuffer) {
            val n = inputBuffer.remaining()
            if (n == 0) return
            val out = replaceOutputBuffer(n)
            if (silent) {
                if (zeros.size < n) zeros = ByteArray(n)
                out.put(zeros, 0, n)
                inputBuffer.position(inputBuffer.limit())
            } else {
                out.put(inputBuffer)
            }
            out.flip()
        }
    }

    /**
     * The leader's sound to Cratarr, as its player plays it: tapped after decoding and gapless trimming, as 16-bit PCM,
     * on a connection of its own to Cratarr's Snapcast port. Cratarr stamps it as it comes.
     */
    class Source : TeeAudioProcessor.AudioBufferSink {
        @Volatile private var active = false
        @Volatile private var target: Triple<String, Int, String>? = null
        private val queue = LinkedBlockingQueue<ByteArray>(QUEUE_MAX)
        private var thread: Thread? = null
        @Volatile private var socket: Socket? = null

        /** The format as the player plays: rate, channels, encoding (playback thread). */
        @Volatile private var format: Triple<Int, Int, Int>? = null
        private var sentFormat: Pair<Int, Int>? = null

        @Synchronized
        fun start(host: String, port: Int, name: String) {
            val t = Triple(host, port, name)
            if (active && target == t) return
            stop()
            target = t
            queue.clear()
            sentFormat = null
            active = true
            thread = Thread({ send(t) }, "cratarr-group-source").apply { isDaemon = true; start() }
        }

        @Synchronized
        fun stop() {
            active = false
            target = null
            thread?.interrupt()
            thread = null
            try {
                socket?.close()
            } catch (_: IOException) {
            }
        }

        /** The sound broke off (playback thread). */
        fun brk() {
            if (active) offer(message(BREAK, ByteArray(0)))
        }

        override fun flush(sampleRateHz: Int, channelCount: Int, encoding: Int) {
            format = Triple(sampleRateHz, channelCount, encoding)
        }

        override fun handleBuffer(buffer: ByteBuffer) {
            if (!active) return
            val (rate, channels, encoding) = format ?: return
            val pcm = toPcm16(buffer.order(ByteOrder.nativeOrder()), encoding) ?: return
            if (sentFormat != rate to channels) {
                sentFormat = rate to channels
                offer(message(CODEC_HEADER, codecHeader(rate, channels)))
            }
            // In chunks of 20 ms at most, so Cratarr can take back what was still to come at a break.
            val step = (rate / 50).coerceAtLeast(1) * channels * 2
            var at = 0
            while (at < pcm.size) {
                val n = minOf(step, pcm.size - at)
                offer(message(WIRE_CHUNK, chunk(pcm, at, n)))
                at += n
            }
        }

        private fun offer(m: ByteArray) {
            if (!queue.offer(m)) {
                // Cratarr out of reach for long: what waited is stale.
                queue.clear()
                queue.offer(message(BREAK, ByteArray(0)))
            }
        }

        private fun send(t: Triple<String, Int, String>) {
            val (host, port, name) = t
            while (active && target == t) {
                try {
                    Socket().use { s ->
                        socket = s
                        s.tcpNoDelay = true
                        s.connect(InetSocketAddress(host, port), 5_000)
                        val out = BufferedOutputStream(s.getOutputStream(), 64 * 1024)
                        // Its name and its secret (`group`'s id: secret, a colon, name).
                        val secret = name.substringBefore(':', "")
                        out.write(message(SOURCE, json(JSONObject().put("player", name.substringAfter(':')).put("secret", secret))))
                        // Afresh: the format first, then what plays from now on.
                        queue.clear()
                        format?.let { (rate, channels, _) -> out.write(message(CODEC_HEADER, codecHeader(rate, channels))) }
                        out.flush()
                        Log.i(TAG, "sending the group's sound to $host:$port")
                        while (active && target == t) {
                            val m = queue.poll(500, TimeUnit.MILLISECONDS) ?: continue
                            out.write(m)
                            if (queue.isEmpty()) out.flush()
                        }
                    }
                } catch (e: IOException) {
                    if (active) Log.w(TAG, "the group's sound to Cratarr: ${e.message}")
                } catch (e: InterruptedException) {
                    return
                }
                try {
                    Thread.sleep(1_000)
                } catch (e: InterruptedException) {
                    return
                }
            }
        }

        private companion object {
            /** About 10 s of sound in 20 ms chunks. */
            const val QUEUE_MAX = 500

            /** 16-bit little-endian PCM of what the player plays; null for an encoding it does not take. */
            fun toPcm16(b: ByteBuffer, encoding: Int): ByteArray? {
                val n = b.remaining()
                return when (encoding) {
                    C.ENCODING_PCM_16BIT -> ByteArray(n).also { b.get(it) }
                    C.ENCODING_PCM_24BIT -> ByteArray(n / 3 * 2).also { out ->
                        for (i in 0 until n / 3) {
                            out[2 * i] = b.get(b.position() + 3 * i + 1)
                            out[2 * i + 1] = b.get(b.position() + 3 * i + 2)
                        }
                    }
                    C.ENCODING_PCM_32BIT -> ByteArray(n / 4 * 2).also { out ->
                        for (i in 0 until n / 4) {
                            out[2 * i] = b.get(b.position() + 4 * i + 2)
                            out[2 * i + 1] = b.get(b.position() + 4 * i + 3)
                        }
                    }
                    C.ENCODING_PCM_FLOAT -> ByteArray(n / 4 * 2).also { out ->
                        for (i in 0 until n / 4) {
                            val v = (b.getFloat(b.position() + 4 * i).coerceIn(-1f, 1f) * 32767f).roundToInt()
                            out[2 * i] = v.toByte()
                            out[2 * i + 1] = (v shr 8).toByte()
                        }
                    }
                    else -> null
                }
            }
        }
    }

    /**
     * Snapcast's player (snapclient, GPL-3.0, as Snapdroid builds it for Android; app/build.gradle.kts fetches it), run as
     * Snapdroid runs it, again whenever it ends while wanted. Wi-Fi in its low-latency mode and the CPU awake meanwhile.
     */
    private class Snapclient(private val context: Context) {
        @Volatile private var wanted: Triple<String, Int, String>? = null
        private var thread: Thread? = null
        @Volatile private var process: Process? = null
        private val wake = context.getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cratarr:group")
            .apply { setReferenceCounted(false) }
        private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
        private val lowLatency = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_LOW_LATENCY, "cratarr:group").apply { setReferenceCounted(false) }

        @Suppress("DEPRECATION") // still what keeps Wi-Fi up with the screen off on older devices
        private val highPerf = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "cratarr:group-perf").apply { setReferenceCounted(false) }

        @android.annotation.SuppressLint("WakelockTimeout") // for as long as the device plays in a group
        @Synchronized
        fun start(host: String, port: Int, id: String) {
            val w = Triple(host, port, id)
            if (wanted == w && thread?.isAlive == true) return
            stop()
            wanted = w
            wake.acquire()
            lowLatency.acquire()
            highPerf.acquire()
            thread = Thread({ run(w) }, "cratarr-snapclient").apply { isDaemon = true; start() }
        }

        @Synchronized
        fun stop() {
            wanted = null
            process?.destroy()
            thread?.interrupt()
            thread = null
            if (wake.isHeld) wake.release()
            if (lowLatency.isHeld) lowLatency.release()
            if (highPerf.isHeld) highPerf.release()
        }

        private fun run(w: Triple<String, Int, String>) {
            val (host, port, id) = w
            while (wanted == w) {
                try {
                    // As Snapdroid: the device's own output rate, so snapclient resamples once, well, and knows its delay.
                    val audio = context.getSystemService(AudioManager::class.java)
                    val rate = audio.getProperty(AudioManager.PROPERTY_OUTPUT_SAMPLE_RATE)
                    val frames = audio.getProperty(AudioManager.PROPERTY_OUTPUT_FRAMES_PER_BUFFER)
                    val pb = ProcessBuilder(
                        context.applicationInfo.nativeLibraryDir + "/libsnapclient.so",
                        "-h", host, "-p", port.toString(), "--hostID", id, "--player", "oboe",
                        "--sampleformat", if (rate != null) "$rate:*:*" else "*:16:*",
                        "--logfilter", "*:info,Stats:debug",
                    ).redirectErrorStream(true)
                    rate?.let { pb.environment()["SAMPLE_RATE"] = it }
                    frames?.let { pb.environment()["FRAMES_PER_BUFFER"] = it }
                    val p = pb.start()
                    process = p
                    Log.i(TAG, "snapclient started for $host:$port")
                    p.inputStream.bufferedReader().forEachLine { Log.d(SNAP_TAG, it) }
                    Log.i(TAG, "snapclient ended: ${p.waitFor()}")
                } catch (e: IOException) {
                    Log.w(TAG, "snapclient: ${e.message}")
                }
                if (wanted != w) return
                try {
                    Thread.sleep(1_000)
                } catch (e: InterruptedException) {
                    return
                }
            }
        }
    }

    companion object {
        private const val TAG = "CratarrSyncPlay"
        private const val SNAP_TAG = "snapclient"

        // Snapcast's binary protocol (its doc/binary_protocol.md), as server/src/snapcast/proto.rs speaks it.
        private const val CODEC_HEADER = 1
        private const val WIRE_CHUNK = 2
        private const val SOURCE = 100
        private const val BREAK = 101

        /** A message: the 26-byte header (times left out: Cratarr stamps what it takes as it comes), then its payload. */
        private fun message(kind: Int, payload: ByteArray): ByteArray = ByteBuffer.allocate(26 + payload.size)
            .order(ByteOrder.LITTLE_ENDIAN)
            .putShort(kind.toShort()).putShort(0).putShort(0)
            .putInt(0).putInt(0).putInt(0).putInt(0)
            .putInt(payload.size)
            .put(payload)
            .array()

        private fun json(o: JSONObject): ByteArray {
            val text = o.toString().toByteArray(Charsets.UTF_8)
            return ByteBuffer.allocate(4 + text.size).order(ByteOrder.LITTLE_ENDIAN).putInt(text.size).put(text).array()
        }

        /** Plain PCM's codec header: "pcm" and a RIFF WAVE header, 16-bit. */
        private fun codecHeader(rate: Int, channels: Int): ByteArray {
            val riff = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
                .put("RIFF".toByteArray()).putInt(36).put("WAVE".toByteArray())
                .put("fmt ".toByteArray()).putInt(16).putShort(1).putShort(channels.toShort())
                .putInt(rate).putInt(rate * 2 * channels).putShort((2 * channels).toShort()).putShort(16)
                .put("data".toByteArray()).putInt(0)
                .array()
            return ByteBuffer.allocate(4 + 3 + 4 + riff.size).order(ByteOrder.LITTLE_ENDIAN)
                .putInt(3).put("pcm".toByteArray()).putInt(riff.size).put(riff)
                .array()
        }

        /** A wire chunk: no stamp (Cratarr stamps it), then the sound. */
        private fun chunk(pcm: ByteArray, from: Int, n: Int): ByteArray = ByteBuffer.allocate(12 + n)
            .order(ByteOrder.LITTLE_ENDIAN)
            .putInt(0).putInt(0).putInt(n)
            .put(pcm, from, n)
            .array()
    }
}
