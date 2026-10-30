/*
 * Track snippets (0.12, snippets: the owner chose them, 2026-10-30): opening a track's menu plays its liveliest 20 s,
 * and the music that plays meanwhile goes down under it. The page picks the stretch and asks (`{"type": "snippet",
 * "play": {source, id, from_ms, to_ms, fade_ms, duck}}` or `{"type": "snippet", "stop": true}`); a small player of its
 * own plays it from Cratarr's stream, faded in and out, while the app's own player is eased to `duck` by volume (a
 * factor [PlayOnLink.applyVolume] multiplies in, beside levelling and Start and stop's), never by audio focus: a focus
 * request would pause the music (the player's focus loss), so the snippet player asks for none. The queue, Play On's
 * target and the music's own focus stay as they are. The page hears where it is (`at_ms`) a few times a second, and
 * `null` as it ends.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.cos
import kotlin.math.log10
import kotlin.math.min
import kotlin.math.pow

/** What the page asks of a snippet: a stretch of a track to play, or to stop. */
sealed class SnippetAsk {
    /** [fromMs] to [toMs] of the track, faded in and out over [fadeMs]; the music at [duck] of its volume meanwhile. */
    data class Play(val source: String, val id: String, val fromMs: Long, val toMs: Long, val fadeMs: Long, val duck: Float) : SnippetAsk() {
        val lengthMs get() = toMs - fromMs
    }

    object Stop : SnippetAsk()

    companion object {
        const val DEFAULT_FADE_MS = 400L
        const val DEFAULT_DUCK = 0.2

        /** The page's message as an ask; null for one that cannot be played (no track, an empty stretch). */
        fun of(m: JSONObject): SnippetAsk? {
            if (m.optBoolean("stop")) return Stop
            val p = m.optJSONObject("play") ?: return null
            fun text(key: String) = p.optString(key).takeIf { it.isNotEmpty() && it != "null" }
            val source = text("source") ?: return null
            val id = text("id") ?: return null
            val from = p.optLong("from_ms", 0L).coerceAtLeast(0L)
            val to = p.optLong("to_ms", -1L)
            if (to <= from) return null
            // Its fades meet in the middle at the most.
            val fade = p.optLong("fade_ms", DEFAULT_FADE_MS).coerceIn(0L, (to - from) / 2)
            val duck = p.optDouble("duck", DEFAULT_DUCK).let { if (it.isNaN()) DEFAULT_DUCK else it }.coerceIn(0.0, 1.0)
            return Play(source, id, from, to, fade, duck.toFloat())
        }
    }
}

/** A snippet's volumes over time: the pure part, so the tests can follow it. */
object SnippetCurve {
    /** Perceptual: 0 silent, 1 full, −60 dB at the quiet end and even in decibels between (as Start and stop's fades). */
    fun loudness(v: Float): Float = if (v <= 0f) 0f else 10f.pow(3f * (min(1f, v) - 1f))

    /** The snippet's own volume [soundedMs] into it: up over [fadeMs] from its start, down over [fadeMs] to its end. */
    fun gain(soundedMs: Long, lengthMs: Long, fadeMs: Long): Float {
        if (soundedMs < 0 || soundedMs >= lengthMs) return 0f
        if (fadeMs <= 0) return 1f
        return loudness(min(soundedMs, lengthMs - soundedMs).toFloat() / fadeMs)
    }

    private fun db(f: Float): Float = 20f * log10(f.coerceAtLeast(1e-4f))

    /** From [from] to [to], [t] ms into [ms]: eased (a raised cosine), even in dB, as levelling's ramp; [to] once done. */
    fun eased(from: Float, to: Float, t: Long, ms: Long): Float {
        if (ms <= 0 || t >= ms) return to
        if (t <= 0) return from
        val e = ((1 - cos(Math.PI * t / ms)) / 2).toFloat()
        val v = 10f.pow((db(from) + (db(to) - db(from)) * e) / 20f)
        // Never past either end (the dB floor stands in for silence).
        return v.coerceIn(min(from, to), maxOf(from, to))
    }
}

/**
 * How long a snippet has sounded, counted while its player plays (a stall for the network stops the count, so the end
 * and the page's playhead stay with what was heard), and when the page is told where it is: at once as it begins to
 * sound, then every [everyMs]. Times are the device's elapsed realtime (ms).
 */
class SnippetClock(private val fromMs: Long, private val lengthMs: Long, private val everyMs: Long = Snippets.REPORT_MS) {
    private var before = 0L
    private var since: Long? = null
    private var told: Long? = null

    /** The player began or stopped sounding at [now]. */
    fun playing(on: Boolean, now: Long) {
        val s = since
        if (on && s == null) since = now
        if (!on && s != null) {
            before += now - s
            since = null
        }
    }

    /** Whether it has sounded at all. */
    val started get() = since != null || before > 0

    fun sounded(now: Long): Long = before + (since?.let { now - it } ?: 0L)

    fun done(now: Long) = sounded(now) >= lengthMs

    /** Where it is in the track. */
    fun atMs(now: Long): Long = fromMs + sounded(now).coerceIn(0L, lengthMs)

    /** Where it is, when the page is due to hear it (it sounds, and [everyMs] have passed since it was told); else null. */
    fun tell(now: Long): Long? {
        if (since == null) return null
        val last = told
        if (last != null && now - last < everyMs) return null
        told = now
        return atMs(now)
    }
}

/**
 * The snippet player, owned by [PlayerService] (whose player's volume it ducks: [applyVolume] sets it again with
 * [duck] in it). Main thread. Its ExoPlayer is made at the first snippet and let go [IDLE_MS] after the last.
 */
@OptIn(UnstableApi::class)
class Snippets(private val context: Context, private val prefs: Prefs, private val applyVolume: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private var player: ExoPlayer? = null
    /** The snippet asked for (loading or sounding), and its count. */
    private var playing: SnippetAsk.Play? = null
    private var clock: SnippetClock? = null
    /** Fading out (stopped, or another one asked for): since when, from what volume, over how long; then [next]. */
    private var outAt: Long? = null
    private var outFrom = 0f
    private var outMs = STOP_MS
    private var next: SnippetAsk.Play? = null
    /** The music's duck as it eases: from, to, since, over; and whether the ease back up at the snippet's end began. */
    private var duckFrom = 1f
    private var duckTo = 1f
    private var duckAt = 0L
    private var duckMs = 0L
    private var duckBack = false

    private fun now() = SystemClock.elapsedRealtime()

    /** A page's snippet message (JSON). */
    fun handle(json: String?) {
        val m = try {
            JSONObject(json.orEmpty())
        } catch (e: Exception) {
            return
        }
        when (val ask = SnippetAsk.of(m)) {
            is SnippetAsk.Play -> play(ask)
            SnippetAsk.Stop -> stop()
            null -> LinkLog.add(context, "warn", "snippet", "cannot be played", "message" to json)
        }
    }

    /** Another snippet while one sounds replaces it: the old one fades out quickly, then the new one starts. */
    private fun play(ask: SnippetAsk.Play) {
        val c = clock
        when {
            playing == null || c == null || !c.started -> start(ask)
            outAt != null -> next = ask
            else -> fadeOut(SWAP_MS, ask)
        }
    }

    private fun stop() {
        next = null
        val c = clock
        when {
            playing == null -> easeDuck(1f, STOP_MS)
            c == null || !c.started -> end(true)
            outAt == null -> {
                fadeOut(STOP_MS, null)
                easeDuck(1f, STOP_MS)
            }
        }
    }

    private fun fadeOut(ms: Long, then: SnippetAsk.Play?) {
        outAt = now()
        outFrom = player?.volume ?: 0f
        outMs = ms
        next = then
        kick()
    }

    private fun start(ask: SnippetAsk.Play) {
        val (item, at) = item(ask) ?: run {
            LinkLog.add(context, "warn", "snippet", "no address", "track" to "${ask.source}/${ask.id}")
            return end(true)
        }
        main.removeCallbacks(letGo)
        val p = player ?: build().also { player = it }
        playing = ask
        clock = SnippetClock(ask.fromMs, ask.lengthMs)
        outAt = null
        next = null
        duckBack = false
        p.volume = 0f
        p.setMediaItem(item, at)
        p.prepare()
        p.play()
        LinkLog.add(context, "info", "snippet", "asked", "track" to "${ask.source}/${ask.id}", "from_ms" to ask.fromMs, "ms" to ask.lengthMs)
    }

    /**
     * The track from the snippet's start: Cratarr's stream begins there (`from_ms`, an MP3 at its frame there too:
     * `mp3_from`), so it plays from 0; on mobile data its smaller copy, as the player's tracks. The device's own track
     * (local mode) from its file, at the place.
     */
    private fun item(ask: SnippetAsk.Play): Pair<MediaItem, Long>? {
        if (ask.source == DeviceMusic.SOURCE) {
            val track = JSONObject().put("source", ask.source).put("id", ask.id)
            val item = mediaItems(JSONArray().put(track)).firstOrNull()?.let(DeviceMusic::located) ?: return null
            return item to ask.fromMs
        }
        val server = prefs.serverUrl?.trimEnd('/') ?: return null
        val opus = if (Net.metered(context)) "&as=opus" else ""
        return MediaItem.fromUri("$server/api/stream/${Uri.encode(ask.source)}/${Uri.encode(ask.id)}?from_ms=${ask.fromMs}&mp3_from=1$opus") to 0L
    }

    /** Ended, stopped or replaced: silent and empty. [last]: nothing follows, so the page hears `null`, the music comes
     *  back up and the player is let go in a while. */
    private fun end(last: Boolean) {
        val was = playing
        player?.let {
            it.volume = 0f
            it.stop()
            it.clearMediaItems()
        }
        playing = null
        clock = null
        outAt = null
        if (!last) return
        next = null
        if (duckTo != 1f) easeDuck(1f, STOP_MS)
        if (was != null) listener?.invoke(null)
        main.removeCallbacks(letGo)
        main.postDelayed(letGo, IDLE_MS)
    }

    private val letGo = Runnable {
        if (playing != null) return@Runnable
        player?.release()
        player = null
    }

    private fun easeDuck(to: Float, ms: Long) {
        val t = now()
        duckFrom = duck
        duckTo = to
        duckAt = t
        duckMs = ms
        kick()
    }

    private fun build(): ExoPlayer {
        val music = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        // No audio focus (false): the app's own player keeps it, and never hears a loss that would pause it.
        // Sounding soon: a quarter of a second in hand is enough to begin.
        val p = ExoPlayer.Builder(context)
            .setAudioAttributes(music, false)
            .setLoadControl(DefaultLoadControl.Builder().setBufferDurationsMs(2_000, 30_000, 250, 1_000).build())
            // Through the app's requests (the address in use, the session): Cratarr's stream; the device's own files
            // through Android.
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(context, Downloads.http(context))))
            .build()
        // Where the music goes (the output chosen, else the owner's order).
        p.setPreferredAudioDevice(Output.preferred(context))
        p.addListener(Heard())
        return p
    }

    /** The snippet player's own course: its count follows its playing; its start brings the duck; its end, the end. */
    private inner class Heard : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            val ask = playing ?: return
            val c = clock ?: return
            val t = now()
            val first = isPlaying && !c.started
            c.playing(isPlaying, t)
            if (!first) return
            // Sounding: the music goes down as it comes up (not before: a slow start would leave a gap).
            easeDuck(ask.duck, ask.fadeMs)
            c.tell(t)?.let { listener?.invoke(it) }
            kick()
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            // The track ended before the stretch did.
            if (playbackState == Player.STATE_ENDED && playing != null) endThenNext()
        }

        override fun onPlayerError(error: PlaybackException) {
            if (playing == null) return
            LinkLog.add(context, "warn", "snippet", "failed", "error" to error.errorCodeName)
            endThenNext()
        }
    }

    /** This one done: the one asked for meanwhile, if any, else the end. */
    private fun endThenNext() {
        val n = next
        end(n == null)
        n?.let(::start)
    }

    /** Every [STEP_MS] while anything moves: the snippet's volume, its end, the page told, the music's duck. */
    private val tick = object : Runnable {
        override fun run() {
            step()
            if (playing != null || duck != duckTo) main.postDelayed(this, STEP_MS)
        }
    }

    private fun kick() {
        main.removeCallbacks(tick)
        main.post(tick)
    }

    private fun step() {
        val t = now()
        val p = player
        val ask = playing
        val c = clock
        if (p != null && ask != null && c != null) {
            val out = outAt
            if (out != null) {
                p.volume = SnippetCurve.eased(outFrom, 0f, t - out, outMs)
                if (t - out >= outMs) endThenNext()
            } else if (c.started) {
                val sounded = c.sounded(t)
                p.volume = SnippetCurve.gain(sounded, ask.lengthMs, ask.fadeMs)
                // The music back up as the snippet fades out.
                if (!duckBack && sounded >= ask.lengthMs - ask.fadeMs) {
                    duckBack = true
                    easeDuck(1f, (ask.lengthMs - sounded).coerceAtLeast(STEP_MS))
                }
                if (c.done(t)) end(true) else c.tell(t)?.let { listener?.invoke(it) }
            }
        }
        val d = SnippetCurve.eased(duckFrom, duckTo, t - duckAt, duckMs)
        if (d != duck) {
            duck = d
            applyVolume()
        }
    }

    fun release() {
        main.removeCallbacksAndMessages(null)
        player?.release()
        player = null
        playing = null
        clock = null
        if (duck != 1f) {
            duck = 1f
            duckTo = 1f
            applyVolume()
        }
    }

    companion object {
        /** The music's share of its volume under a snippet (1: none): [PlayOnLink.applyVolume] multiplies it in. */
        @Volatile
        var duck = 1f
            private set

        /** The page's playhead: where the snippet is (ms into the track), or null as it ends. Set by [PagePlayer]. */
        @Volatile
        var listener: ((Long?) -> Unit)? = null

        /** A stop's fade; another snippet's quicker one before it starts. */
        const val STOP_MS = 300L
        const val SWAP_MS = 120L
        /** Volumes stepped this often; the page told this often. */
        const val STEP_MS = 30L
        const val REPORT_MS = 250L
        /** The player let go this long after the last snippet (menus opened one after another reuse it). */
        const val IDLE_MS = 15_000L
    }
}
