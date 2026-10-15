/*
 * Changed for Cratarr, 2026-10-15: in the place of Jellyfin's RemotePlayerService (jellyfin-android, GPL-2.0), which
 * showed the page's playback on the lock screen. Here the app plays itself (decision 0031, amended): Media3's player in
 * a media session service, which brings the lock screen, the notification, headphone buttons, audio focus, pausing
 * when the audio would become noisy, and the wake and Wi-Fi locks while it plays. It also holds this player's Play On
 * connection to Cratarr (PlayOnLink), and with Stay ready stays in the foreground with nothing queued too. Tracks are
 * read through the device's store (Downloads): a downloaded one, or one of the next few, plays without a connection.
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
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.annotation.OptIn
import androidx.core.content.ContextCompat
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

@OptIn(UnstableApi::class)
class PlayerService : MediaSessionService() {
    private var session: MediaSession? = null
    private val main = Handler(Looper.getMainLooper())
    private var link: PlayOnLink? = null
    private lateinit var prefs: Prefs
    private lateinit var notifications: NotificationManager
    private lateinit var player: ExoPlayer

    /** The server's address or the player's name changed: Play On connects again with them. A sleep timer set: kept. */
    private val prefsChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == Prefs.SERVER_URL || key == Prefs.PLAYER_NAME || key == Prefs.STAY_READY) link?.reconnect()
        if (key == Prefs.SLEEP) {
            keepSleep()
            link?.sleepChanged()
        }
    }

    /** The sleep timer's time, looked at again when it should have come (the device may have slept meanwhile). */
    private val sleepAt = Runnable { sleepCheck() }

    override fun onCreate() {
        super.onCreate()
        prefs = Prefs(this)
        notifications = getSystemService(NotificationManager::class.java)
        val music = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        player = ExoPlayer.Builder(this)
            .setAudioAttributes(music, true)
            .setHandleAudioBecomingNoisy(true)
            // A wake lock and a Wi-Fi lock while it plays: the screen off stops neither the music nor its download.
            .setWakeMode(C.WAKE_MODE_NETWORK)
            // Previous restarts the track after this long in, as the page's player does (Finamp: 5 s).
            .setMaxSeekToPreviousPositionMs(RESTART_AFTER_MS)
            // Through the device's store first: downloads and the next tracks play from there.
            .setMediaSourceFactory(DefaultMediaSourceFactory(Downloads.playback(this)))
            .build()
        player.addListener(Keeper(player))
        Repeat.apply(this, player)
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
            .build()
        link = PlayOnLink(this, player, prefs).also { it.start() }
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(prefsChanged)
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_READY) {
            val idle = session?.player?.mediaItemCount == 0
            if (prefs.stayReady && idle) showReady()
            // Not ready (any more), nothing queued: nothing keeps it.
            if (!prefs.stayReady && idle) {
                stopForeground(STOP_FOREGROUND_REMOVE)
                notifications.cancel(READY_NOTIFICATION)
                stopSelf()
            }
            return START_STICKY
        }
        return super.onStartCommand(intent, flags, startId)
    }

    /**
     * Stay ready: in the foreground also while paused, and with nothing queued its own "Ready" notification, so Android
     * does not put the app to sleep and Play On still reaches it.
     */
    override fun onUpdateNotification(session: MediaSession, startInForegroundRequired: Boolean) {
        val stay = prefs.stayReady
        if (stay && session.player.mediaItemCount == 0) {
            showReady()
            notifications.cancel(DefaultMediaNotificationProvider.DEFAULT_NOTIFICATION_ID)
            return
        }
        notifications.cancel(READY_NOTIFICATION)
        super.onUpdateNotification(session, startInForegroundRequired || stay)
    }

    private fun showReady() {
        notifications.createNotificationChannel(NotificationChannel(READY_CHANNEL, getString(R.string.ready), NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val ready = Notification.Builder(this, READY_CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(getString(R.string.ready))
            .setContentText(prefs.playerName)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        try {
            startForeground(READY_NOTIFICATION, ready, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } catch (e: IllegalStateException) {
            // Android refuses a foreground start from the background; the next start from the app shows it.
            Log.w(TAG, "cannot stay ready from the background: ${e.message}")
        }
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
                    player.currentMediaItem?.let(::trackOf)?.let { Plays.played(this@PlayerService, it) }
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
            // As a player does: what cannot play (gone, or out of reach offline) is skipped, unless it was the last.
            if (player.hasNextMediaItem()) {
                player.seekToNextMediaItem()
                player.prepare()
                player.play()
            }
        }
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(prefsChanged)
        link?.stop()
        link = null
        session?.run {
            player.release()
            release()
        }
        session = null
        super.onDestroy()
    }

    /** Tracks arrive by their id only; the file and the cover come from the server. */
    private inner class Callback : MediaSession.Callback {
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> = Futures.immediateFuture(mediaItems.map(::located).toMutableList())
    }

    /** A track's item with its file (a download's, else `/api/file/{source}/{id}`) and its cover from the server. */
    fun located(item: MediaItem): MediaItem {
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
        /** How often the playing track is looked at for counting. */
        private const val COUNT_EVERY_MS = 5_000L
        private const val READY_CHANNEL = "ready"
        private const val READY_NOTIFICATION = 7
        private const val ACTION_READY = "cratarr.READY"

        /** Started (or let go) as Stay ready says. Called while the app is in front, where a foreground start is allowed. */
        fun ready(context: Context) {
            val intent = Intent(context, PlayerService::class.java).setAction(ACTION_READY)
            try {
                if (Prefs(context).stayReady) ContextCompat.startForegroundService(context, intent) else context.startService(intent)
            } catch (e: IllegalStateException) {
                Log.w(TAG, "cannot start staying ready: ${e.message}")
            }
        }
    }
}
