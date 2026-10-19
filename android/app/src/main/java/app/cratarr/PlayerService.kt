/*
 * Changed for Cratarr, 2026-10-15: in the place of Jellyfin's RemotePlayerService (jellyfin-android, GPL-2.0), which
 * showed the page's playback on the lock screen. Here the app plays itself (decision 0031, amended): Media3's player in
 * a media session service, which brings the lock screen, the notification, headphone buttons, audio focus, pausing
 * when the audio would become noisy, and the wake and Wi-Fi locks while it plays. It also holds this player's Play On
 * connection to Cratarr (PlayOnLink); with Stay ready the app stays in the foreground by a service of its own (Ready).
 * Tracks are read through the device's store (Downloads): a downloaded one, or one of the next few, plays without a
 * connection. In local mode (decision 0052) the same player plays the device's own music (DeviceMusic), with a queue of
 * its own.
 */
package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.CacheBitmapLoader
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import org.json.JSONException
import org.json.JSONObject

@OptIn(UnstableApi::class)
class PlayerService : MediaSessionService() {
    private var session: MediaSession? = null
    private val main = Handler(Looper.getMainLooper())
    private var link: PlayOnLink? = null
    private lateinit var prefs: Prefs
    private lateinit var notifications: NotificationManager
    private lateinit var player: ExoPlayer
    /** Sync play (decision 0037): this device in a group. */
    private lateinit var sync: SyncPlay

    /** The server's address or the player's name changed: Play On connects again with them. A sleep timer set: kept. */
    private val prefsChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        when (key) {
            Prefs.IN_USE -> link?.reconnect("address changed")
            Prefs.PLAYER_NAME -> link?.reconnect("name changed")
            Prefs.STAY_READY -> {
                link?.reconnect("stay ready changed")
                Ready.ensure(this, "stay ready changed")
            }
            Prefs.SESSION_MARK -> link?.reconnect("session changed")
        }
        // An address set: which of the two answers.
        if (key == Prefs.SERVER_URL || key == Prefs.LOCAL_URL || key == Prefs.PREFER_LOCAL || key == Prefs.SESSION_MARK) Reach.checkSoon(this)
        // Another server or name: Cratarr says the group again once connected.
        if ((key == Prefs.SERVER_URL || key == Prefs.LOCAL_URL || key == Prefs.PLAYER_NAME) && sync.leader != null) sync.group(null, null, 0, sync.bufferMs, "")
        if (key == Prefs.LEVELLING) link?.levellingChanged()
        if (key == Prefs.OUTPUT_CHOICE || key == Prefs.OUTPUT_ORDERED || key == Prefs.OUTPUT_KNOWN) applyOutput()
        if (key == Prefs.SLEEP) {
            keepSleep()
            link?.sleepChanged()
        }
        // Radio or Autoplay set (here, by the page or by Cratarr): Cratarr is told; the playing track gets its inserts,
        // the queue its next ones when it runs low.
        if (key == Prefs.RADIO_ON || key == Prefs.RADIO_BY || key == Prefs.RADIO_COUNT || key == Prefs.AUTOPLAY || key == Prefs.SKIP_FAVOURITES) {
            link?.sleepChanged()
            Radio.insert(this, player, prefs.serverUrl, ::located)
            Autoplay.fill(this, player, prefs.serverUrl, ::located)
        }
    }

    /** The sleep timer's time, looked at again when it should have come (the device may have slept meanwhile). */
    private val sleepAt = Runnable { sleepCheck() }

    override fun onCreate() {
        super.onCreate()
        // Its own log (decision 0045): the service's start, and a crash, written down.
        LinkLog.catchCrashes(this)
        LinkLog.add(this, "info", "playon", "service started")
        prefs = Prefs(this)
        notifications = getSystemService(NotificationManager::class.java)
        val music = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        sync = SyncPlay(this, prefs)
        player = ExoPlayer.Builder(this, sync.renderers(this))
            .setAudioAttributes(music, true)
            .setHandleAudioBecomingNoisy(true)
            // A wake lock and a Wi-Fi lock while it plays: the screen off stops neither the music nor its download.
            .setWakeMode(C.WAKE_MODE_NETWORK)
            // Previous restarts the track after this long in, as the page's player does (Finamp: 5 s).
            .setMaxSeekToPreviousPositionMs(RESTART_AFTER_MS)
            // Through the device's store first: downloads and the next tracks play from there. The device's own files
            // (local mode, content://) through Android.
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(this, Downloads.playback(this))).setLoadErrorHandlingPolicy(Patience()))
            .build()
        player.addListener(Keeper(player))
        sync.attach(player)
        sync.onChange = { main.post(::groupChanged) }
        Repeat.apply(this, player)
        prefs.migrate()
        // A sleep timer from before: its music is gone.
        Sleep.clear(this)
        // Previous from the lock screen, the notification and the page goes through here too: under repeat all, the
        // first track's previous is the last one.
        val sessionPlayer = object : ForwardingPlayer(player) {
            override fun seekToPrevious() {
                if (!Repeat.previous(this@PlayerService, player)) super.seekToPrevious()
            }
        }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        session = MediaSession.Builder(this, sessionPlayer)
            .setSessionActivity(open)
            .setCallback(Callback())
            // Covers (the notification's, the lock screen's, a car's) through the app's own requests, with its session:
            // away from home the proxy refused Media3's own without it (2026-10-19). The device's own albums' from Android.
            .setBitmapLoader(
                CacheBitmapLoader(
                    DeviceMusic.Covers(this, DataSourceBitmapLoader.Builder(this).setDataSourceFactory(DefaultDataSource.Factory(this, Downloads.http(this))).build()),
                ),
            )
            .build()
        // Local mode (decision 0052) as it was: its queue again, paused; without Android's permission or a folder turned
        // on any more, Cratarr's.
        if (prefs.mode == Prefs.MODE_DEVICE) {
            if (DeviceMusic.on(this)) restore(prefs.savedQueue(Prefs.MODE_DEVICE)) else prefs.mode = Prefs.MODE_CRATARR
        }
        link = PlayOnLink(this, player, prefs, sync).also { it.start() }
        Ready.ensure(this, "service started")
        applyOutput()
        Reach.watch(this)
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(prefsChanged)
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_READY) {
            // The app in front: ready (again), and what Android held back plays now.
            Ready.ensure(this, "app opened")
            link?.playHeld()
            // Not ready, nothing queued: nothing keeps it.
            if (!prefs.stayReady && session?.player?.mediaItemCount == 0) stopSelf()
            return START_STICKY
        }
        return super.onStartCommand(intent, flags, startId)
    }

    /**
     * Stay ready: a paused queue's notification in the foreground too; with nothing queued, the Ready service's own
     * notification is the one there is.
     */
    override fun onUpdateNotification(session: MediaSession, startInForegroundRequired: Boolean) {
        // Following a group: its own notification, which keeps the app (and Snapcast's player in it) running.
        if (sync.follows) {
            showGroup()
            notifications.cancel(DefaultMediaNotificationProvider.DEFAULT_NOTIFICATION_ID)
            return
        }
        notifications.cancel(GROUP_NOTIFICATION)
        super.onUpdateNotification(session, startInForegroundRequired || (prefs.stayReady && session.player.mediaItemCount > 0))
    }

    /** Following a group began or ended. */
    private fun groupChanged() {
        if (sync.follows) showGroup() else session?.let { onUpdateNotification(it, false) }
    }

    private fun showGroup() {
        notifications.createNotificationChannel(NotificationChannel(READY_CHANNEL, getString(R.string.ready), NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val grouped = Notification.Builder(this, READY_CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(getString(R.string.grouped, sync.leader ?: ""))
            .setContentText(prefs.playerName)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        try {
            startForeground(GROUP_NOTIFICATION, grouped, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } catch (e: IllegalStateException) {
            Log.w(TAG, "cannot stay in front while grouped: ${e.message}")
        }
    }

    /**
     * Local mode (decision 0052): Cratarr's music or the device's own, never mixed. Each keeps its queue: the one left
     * (its tracks, the one that plays and where, shuffle and repeat) is put by, and the other one taken up where it was,
     * paused. Entering local mode leaves a group. True when the mode changed.
     */
    fun switchMode(to: String, why: String): Boolean {
        val mode = if (to == Prefs.MODE_DEVICE) Prefs.MODE_DEVICE else Prefs.MODE_CRATARR
        val was = prefs.mode
        if (was == mode || (mode == Prefs.MODE_DEVICE && !DeviceMusic.on(this))) return false
        LinkLog.add(this, "info", "playon", "mode", "to" to mode, "why" to why)
        if (mode == Prefs.MODE_DEVICE && sync.leader != null) sync.leave()
        player.pause()
        prefs.saveQueue(was, snapshot().toString())
        prefs.mode = mode
        restore(prefs.savedQueue(mode))
        link?.modeChanged()
        return true
    }

    /** The queue as it is now, for [restore]. */
    private fun snapshot(): JSONObject = JSONObject()
        .put("tracks", queueOf(player))
        .put("index", player.currentMediaItemIndex)
        .put("position_ms", player.currentPosition)
        .put("repeat", prefs.repeat)
        .put("shuffled", prefs.shuffled)

    /** A queue put by ([snapshot]) in the player, paused; none (empty): nothing queued. */
    private fun restore(saved: String) {
        val s = try {
            JSONObject(saved)
        } catch (e: JSONException) {
            JSONObject()
        }
        prefs.repeat = s.optString("repeat", Repeat.OFF)
        prefs.shuffled = s.optBoolean("shuffled")
        Repeat.apply(this, player)
        val items = mediaItems(s.optJSONArray("tracks")).map(::located)
        if (items.isEmpty()) {
            player.stop()
            player.clearMediaItems()
            return
        }
        player.setMediaItems(items, s.optInt("index").coerceIn(0, items.size - 1), s.optLong("position_ms").coerceAtLeast(0))
        player.prepare()
    }

    /** Local mode's queue kept as it goes, for a restart (changes close together as one). */
    private val saveDevice = Runnable { if (prefs.mode == Prefs.MODE_DEVICE) prefs.saveQueue(Prefs.MODE_DEVICE, snapshot().toString()) }

    /**
     * This device's output as chosen in its sheet (decision 0033, amended 2026-10-19): the player's sound goes there,
     * alone or leading a group (Snapcast's own player, following one, takes none: Android routes it). A choice lasts
     * until the outputs there are change (a Bluetooth device comes or goes): then it is Automatic again, so the speaker
     * picked in the car is not picked the next time.
     */
    fun outputsChanged() {
        if (prefs.outputChoice.isNotEmpty() && Output.set(this) != prefs.outputChoiceSet) {
            LinkLog.add(this, "info", "playon", "output choice ended", "was" to prefs.outputChoice)
            prefs.outputChoice = ""
        }
        applyOutput()
    }

    /** The chosen output, else the first of the owner's order that is there ([Output.preferred]); picked again as
     *  outputs come and go. */
    private fun applyOutput() {
        player.setPreferredAudioDevice(Output.preferred(this))
    }

    /** The sleep timer as set: the end of the track by the player itself, the time by a wait. */
    private fun keepSleep() {
        val timer = Sleep.get(this)
        player.pauseAtEndOfMediaItems = timer?.endOfTrack == true
        main.removeCallbacks(sleepAt)
        timer?.atMs?.let { main.postDelayed(sleepAt, (it - System.currentTimeMillis()).coerceAtLeast(0)) }
    }

    private fun sleepCheck() {
        val at = Sleep.get(this)?.atMs ?: return
        val left = at - System.currentTimeMillis()
        if (left > 0) {
            main.postDelayed(sleepAt, left)
            return
        }
        // Cleared first, so what the pause reports has no timer.
        Sleep.clear(this)
        player.pause()
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        LinkLog.add(this, "info", "playon", "app swiped away")
        // The app swiped away: music that plays goes on, and so does a player that stays ready; otherwise it goes.
        val player = session?.player
        if (!prefs.stayReady && (player == null || !player.playWhenReady || player.mediaItemCount == 0)) stopSelf()
    }

    /**
     * Keeps the next tracks on the device as the queue moves on, counts the plays (Plays), and skips a track that cannot
     * be played.
     */
    private inner class Keeper(private val player: ExoPlayer) : Player.Listener {
        private val keep = Runnable {
            // Local mode's tracks are on the device already.
            if (prefs.mode == Prefs.MODE_DEVICE) return@Runnable
            val items = (0 until player.mediaItemCount).map(player::getMediaItemAt)
            Downloads.keepAhead(this@PlayerService, items, player.currentMediaItemIndex)
        }

        /** The playing track has been counted, this time it plays. */
        private var counted = false
        private val count = object : Runnable {
            override fun run() {
                val d = player.duration
                if (!counted && d > 0 && player.currentPosition >= d * Plays.PLAYED_AT) {
                    counted = true
                    // The device's own music is not counted (decision 0052).
                    player.currentMediaItem?.takeUnless(DeviceMusic::owns)?.let(::trackOf)?.let { Plays.played(this@PlayerService, it) }
                }
                if (player.isPlaying) main.postDelayed(this, COUNT_EVERY_MS)
            }
        }

        override fun onEvents(player: Player, events: Player.Events) {
            if (events.containsAny(Player.EVENT_TIMELINE_CHANGED, Player.EVENT_MEDIA_ITEM_TRANSITION)) {
                // Changes close together (a queue being built) as one.
                main.removeCallbacks(keep)
                main.postDelayed(keep, 1_000)
                // Repeat all: the last track plays, the next round goes after it.
                Repeat.extend(this@PlayerService, player, ::located)
                // Radio: the playing own track's inserts; Autoplay: fewer than three ahead, the next ones (decision 0036).
                Radio.insert(this@PlayerService, player, prefs.serverUrl, ::located)
                Autoplay.fill(this@PlayerService, player, prefs.serverUrl, ::located)
            }
            if (prefs.mode == Prefs.MODE_DEVICE && events.containsAny(
                    Player.EVENT_TIMELINE_CHANGED,
                    Player.EVENT_MEDIA_ITEM_TRANSITION,
                    Player.EVENT_PLAY_WHEN_READY_CHANGED,
                    Player.EVENT_POSITION_DISCONTINUITY,
                )
            ) {
                main.removeCallbacks(saveDevice)
                main.postDelayed(saveDevice, 1_000)
            }
            // Each time a track is reached it can count again (a repeat, a jump back).
            if (events.contains(Player.EVENT_MEDIA_ITEM_TRANSITION)) counted = false
            if (events.contains(Player.EVENT_IS_PLAYING_CHANGED)) {
                main.removeCallbacks(count)
                if (player.isPlaying) main.post(count)
                // A sleep timer that ran out while the music was paused only clears (its wait may not have come yet:
                // the device sleeps while nothing plays).
                val at = Sleep.get(this@PlayerService)?.atMs
                if (player.isPlaying && at != null && at <= System.currentTimeMillis()) Sleep.clear(this@PlayerService)
            }
            // Nothing queued any more: no timer.
            if (events.contains(Player.EVENT_TIMELINE_CHANGED) && player.mediaItemCount == 0 && Sleep.get(this@PlayerService) != null) {
                Sleep.clear(this@PlayerService)
            }
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            // The track the sleep timer waited for ended: the player paused itself, and the timer is done.
            if (reason == Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) Sleep.clear(this@PlayerService)
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_ENDED && Sleep.get(this@PlayerService)?.endOfTrack == true) Sleep.clear(this@PlayerService)
        }

        override fun onPlayerError(error: PlaybackException) {
            val cause = error.cause
            val status = (cause as? HttpDataSource.InvalidResponseCodeException)?.responseCode
            // Cratarr still out of reach after a minute of asking (Patience): the same track again in a while, where it
            // was. Refused (sign in again): every track would be, so it waits for the sign-in (Net tells the owner).
            // Otherwise, as a player does: what cannot play (gone, or out of reach offline) is skipped, unless it was
            // the last.
            val action = when {
                // The device's music, no longer allowed: every track would fail.
                player.currentMediaItem?.let(DeviceMusic::owns) == true && !DeviceMusic.granted(this@PlayerService) -> "stop"
                Patience.refused(cause) -> "wait for sign-in"
                Patience.waitFor(cause) -> "again"
                player.hasNextMediaItem() -> "skip"
                else -> "stop"
            }
            LinkLog.add(
                this@PlayerService, "warn", "playon", "track failed",
                "track" to player.currentMediaItem?.mediaId, "error" to error.errorCodeName, "status" to status, "action" to action,
            )
            when (action) {
                "again" -> main.postDelayed({ if (player.playerError != null) player.prepare() }, AGAIN_MS)
                "skip" -> {
                    player.seekToNextMediaItem()
                    player.prepare()
                    player.play()
                }
            }
        }
    }

    override fun onDestroy() {
        LinkLog.add(this, "info", "playon", "service stopped")
        main.removeCallbacksAndMessages(null)
        saveDevice.run()
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(prefsChanged)
        sync.release()
        link?.stop()
        link = null
        session?.run {
            player.release()
            release()
        }
        session = null
        super.onDestroy()
    }

    /** Tracks arrive by their id only; the file and the cover come from the server. The app's own page may switch the
     *  mode ([COMMAND_MODE], `mode`). */
    private inner class Callback : MediaSession.Callback {
        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
            if (controller.packageName != packageName) return super.onConnect(session, controller)
            // Media3's own defaults for a controller of the app's (every player command), and the mode. Not built on
            // super's answer: that is a stand-in with no commands at all, which Media3 replaces only when it is returned
            // as it is (copied, the page's player could do nothing: 69, fixed in 69.1).
            val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon().add(SessionCommand(COMMAND_MODE, Bundle.EMPTY)).build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session, controller).setAvailableSessionCommands(commands).build()
        }

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            if (customCommand.customAction != COMMAND_MODE) return super.onCustomCommand(session, controller, customCommand, args)
            switchMode(args.getString("mode").orEmpty(), "page")
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> = Futures.immediateFuture(mediaItems.map(::located).toMutableList())
    }

    /** A track's item with its file (a download's, else `/api/file/{source}/{id}`) and its cover from the server; the
     *  device's own from Android. */
    fun located(item: MediaItem): MediaItem {
        DeviceMusic.located(item)?.let { return it }
        val server = prefs.serverUrl?.trimEnd('/')
        val found = Downloads.locate(this, item, server)
        server ?: return found
        return found.buildUpon()
            .setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(Uri.parse("$server/api/cover/${item.mediaId}")).build())
            .build()
    }

    companion object {
        private const val TAG = "CratarrPlayer"
        private const val RESTART_AFTER_MS = 5_000L
        /** A track Cratarr could not give for a minute: asked for again after this long. */
        private const val AGAIN_MS = 15_000L
        /** How often the playing track is looked at for counting. */
        private const val COUNT_EVERY_MS = 5_000L
        private const val READY_CHANNEL = "ready"
        private const val GROUP_NOTIFICATION = 8
        private const val ACTION_READY = "cratarr.READY"
        /** The page's mode switch (decision 0052): `mode`, cratarr or device. */
        const val COMMAND_MODE = "cratarr.MODE"

        /** Started, and ready (or let go) as Stay ready says. Called while the app is in front, where both are allowed. */
        fun ready(context: Context) {
            try {
                context.startService(Intent(context, PlayerService::class.java).setAction(ACTION_READY))
            } catch (e: IllegalStateException) {
                Log.w(TAG, "cannot start: ${e.message}")
            }
            Ready.ensure(context, "app opened")
        }
    }
}
