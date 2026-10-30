package app.cratarr

import android.net.Uri
import android.os.Looper
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.DeviceInfo
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.util.concurrent.Executors

/**
 * One of Cratarr's players that this device's screen drives (the owner, 2026-10-23: the phone's own media controls said
 * "This phone" and a paused track while its screen drove the Salon tablet). While the page plays on another player, the
 * app's media session shows this one instead of its own player: Android's notification, lock screen and output chip
 * show what plays there, and their play, pause, next, previous, seek and the volume keys go there as Play On commands.
 * Its playback is remote (`DeviceInfo.PLAYBACK_TYPE_REMOTE`, volume 0 to 100), so Android shows it as played elsewhere.
 *
 * What plays there comes from Cratarr's live channel of its players (`/api/events/players`), read while it is driven;
 * the position is counted on from the report's moment on Cratarr's clock ([Clock]).
 */
@OptIn(UnstableApi::class)
class RemotePlayer(private val service: PlayerService, private val prefs: Prefs) : SimpleBasePlayer(Looper.getMainLooper()) {
    private val main = android.os.Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private val commands = Executors.newSingleThreadExecutor()

    /** The player driven, by name; null: none. */
    @Volatile var name: String? = null
        private set
    /** Its last view (`/api/players`' entry); null until heard. */
    private var view: JSONObject? = null
    /** Whether a view of it was heard since it was pointed at, and when its music last moved as that said (epoch ms). */
    private var heardOnce = false
    private var movedAt: Long? = null
    /** The reading of the live channel under way: a newer one ends it. */
    @Volatile private var reading = 0
    @Volatile private var connection: java.net.HttpURLConnection? = null

    /** Drives [player] from now on (null: none), reading what plays there. */
    fun follow(player: String?) {
        if (player == name) return
        name = player
        view = null
        heardOnce = false
        movedAt = null
        val run = ++reading
        connection?.disconnect()
        invalidateState()
        if (player != null) io.execute { listen(run) }
        LinkLog.add(service, "info", "playon", "media controls", "drive" to player)
    }

    private fun listen(run: Int) {
        var wait = 2_000L
        while (run == reading) {
            val server = prefs.serverUrl?.trimEnd('/')
            if (server == null) {
                sleep(10_000)
                continue
            }
            try {
                val c = Net.open(service, "$server/api/events/players")
                connection = c
                c.connectTimeout = 10_000
                c.readTimeout = 90_000
                c.setRequestProperty("Accept", "text/event-stream")
                if (Net.code(service, c) == 200) {
                    wait = 2_000L
                    read(run, BufferedReader(InputStreamReader(c.inputStream, Charsets.UTF_8)))
                }
            } catch (e: Exception) {
                if (run == reading) Log.w(TAG, "players' channel: ${e.message}")
            } finally {
                connection?.disconnect()
                connection = null
            }
            if (run != reading) return
            sleep(wait)
            wait = (wait * 2).coerceAtMost(60_000L)
        }
    }

    private fun read(run: Int, reader: BufferedReader) {
        var event = ""
        val data = StringBuilder()
        while (run == reading) {
            val line = reader.readLine() ?: return
            when {
                line.isEmpty() -> {
                    if (event == "players" && data.isNotEmpty()) {
                        val json = data.toString()
                        main.post { heard(run, json) }
                    }
                    event = ""
                    data.setLength(0)
                }
                line.startsWith("event:") -> event = line.removePrefix("event:").trim()
                line.startsWith("data:") -> {
                    if (data.isNotEmpty()) data.append('\n')
                    data.append(line.removePrefix("data:").removePrefix(" "))
                }
            }
        }
    }

    private fun heard(run: Int, json: String) {
        if (run != reading) return
        val players = try {
            JSONObject(json).optJSONArray("players")
        } catch (e: Exception) {
            null
        } ?: return
        view = (0 until players.length()).map(players::getJSONObject).firstOrNull { it.optString("name") == name }
        // Its music handed over to another player (the owner, 2026-10-30: "every screen showing that player"): the
        // controls go there, as if it was picked. Not for a move its first view already says (pointed here since).
        val moved = MovedTo.ofView(view)
        val news = heardOnce && moved != null && moved.second != movedAt
        heardOnce = true
        movedAt = moved?.second
        invalidateState()
        if (news && moved != null) service.pointAt(moved.first, "its music moved there")
    }

    override fun getState(): State {
        val v = view
        val track = v?.optJSONObject("track")
        val base = State.Builder()
            .setDeviceInfo(DeviceInfo.Builder(DeviceInfo.PLAYBACK_TYPE_REMOTE).setMinVolume(0).setMaxVolume(100).build())
            .setDeviceVolume(v?.optInt("volume", 100) ?: 100)
        if (v == null || track == null || !v.optBoolean("online")) {
            return base.setAvailableCommands(Player.Commands.Builder().add(Player.COMMAND_GET_DEVICE_VOLUME).build())
                .setPlaybackState(Player.STATE_IDLE)
                .build()
        }
        val server = prefs.serverUrl?.trimEnd('/')
        val source = track.optString("source")
        val id = track.optString("id")
        val metadata = MediaMetadata.Builder()
            .setTitle(track.optString("name").ifEmpty { v.optString("title") })
            .setArtist(track.optString("artist").ifEmpty { v.optString("artist") })
            .setAlbumTitle(track.optString("album").ifEmpty { v.optString("album") })
            .apply { if (server != null && id.isNotEmpty()) setArtworkUri(Uri.parse("$server/api/cover/$source/$id")) }
            .build()
        val item = MediaItem.Builder().setMediaId("$source/$id").setMediaMetadata(metadata).build()
        val durationMs = v.optLong("duration_ms", -1)
        val playing = v.optBoolean("playing")
        val at = v.optLong("position_at", 0)
        val since = if (playing && at > 0) (Clock.now() - at).toLong().coerceAtLeast(0) else 0
        val position = (v.optLong("position_ms") + since).let { if (durationMs > 0) it.coerceAtMost(durationMs) else it }
        val index = v.optInt("index", 0)
        val length = v.optInt("queue_length", 1)
        val available = Player.Commands.Builder()
            .addAll(
                Player.COMMAND_PLAY_PAUSE,
                Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
                Player.COMMAND_GET_METADATA,
                Player.COMMAND_GET_TIMELINE,
                Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
                Player.COMMAND_SEEK_TO_PREVIOUS,
                Player.COMMAND_GET_DEVICE_VOLUME,
                Player.COMMAND_SET_DEVICE_VOLUME_WITH_FLAGS,
                Player.COMMAND_ADJUST_DEVICE_VOLUME_WITH_FLAGS,
            )
            .apply { if (index + 1 < length) add(Player.COMMAND_SEEK_TO_NEXT) }
            .build()
        return base
            .setAvailableCommands(available)
            .setPlaylist(
                listOf(
                    MediaItemData.Builder(track.optString("entry").ifEmpty { "$source/$id" })
                        .setMediaItem(item)
                        .setDurationUs(if (durationMs > 0) durationMs * 1000 else C.TIME_UNSET)
                        .setIsSeekable(true)
                        .build(),
                ),
            )
            .setCurrentMediaItemIndex(0)
            .setPlaybackState(Player.STATE_READY)
            .setPlayWhenReady(playing, Player.PLAY_WHEN_READY_CHANGE_REASON_REMOTE)
            .setContentPositionMs(PositionSupplier.getExtrapolating(position, if (playing) 1f else 0f))
            .build()
    }

    /** A Play On command to the player driven; done when Cratarr answered (its report then comes on the channel). */
    private fun command(body: JSONObject): ListenableFuture<*> {
        val player = name ?: return Futures.immediateVoidFuture()
        val server = prefs.serverUrl?.trimEnd('/') ?: return Futures.immediateVoidFuture()
        val done = SettableFuture.create<Unit>()
        commands.execute {
            try {
                val c = Net.open(service, "$server/api/players/${Uri.encode(player)}/command")
                c.requestMethod = "POST"
                c.connectTimeout = 5_000
                c.readTimeout = 5_000
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
                val code = Net.code(service, c)
                c.disconnect()
                if (code !in 200..299) LinkLog.add(service, "warn", "playon", "media control refused", "player" to player, "command" to body.optString("command"), "status" to code)
            } catch (e: Exception) {
                LinkLog.add(service, "warn", "playon", "media control failed", "player" to player, "command" to body.optString("command"), "error" to e.message)
            }
            done.set(Unit)
        }
        return done
    }

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> =
        command(JSONObject().put("command", if (playWhenReady) "resume" else "pause"))

    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> = when (seekCommand) {
        Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> command(JSONObject().put("command", "next"))
        Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM -> command(JSONObject().put("command", "previous"))
        else -> command(JSONObject().put("command", "seek").put("position_ms", positionMs.coerceAtLeast(0)))
    }

    override fun handleSetDeviceVolume(deviceVolume: Int, flags: Int): ListenableFuture<*> =
        command(JSONObject().put("command", "volume").put("level", deviceVolume.coerceIn(0, 100)))

    override fun handleIncreaseDeviceVolume(flags: Int): ListenableFuture<*> = stepVolume(VOLUME_STEP)

    override fun handleDecreaseDeviceVolume(flags: Int): ListenableFuture<*> = stepVolume(-VOLUME_STEP)

    private fun stepVolume(by: Int): ListenableFuture<*> {
        val now = view?.optInt("volume", 100) ?: 100
        return handleSetDeviceVolume((now + by).coerceIn(0, 100), 0)
    }

    override fun handleRelease(): ListenableFuture<*> {
        follow(null)
        io.shutdownNow()
        commands.shutdown()
        return Futures.immediateVoidFuture()
    }

    private fun sleep(ms: Long) {
        try {
            Thread.sleep(ms)
        } catch (_: InterruptedException) {
        }
    }

    companion object {
        private const val TAG = "CratarrRemote"
        /** A volume key's step on the player driven (of 100). */
        private const val VOLUME_STEP = 5
    }
}
