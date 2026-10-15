/*
 * Changed for Cratarr, 2026-10-15: in the place of Jellyfin's RemotePlayerService (jellyfin-android, GPL-2.0), which
 * showed the page's playback on the lock screen. Here the app plays itself (decision 0031, amended): Media3's player in
 * a media session service, which brings the lock screen, the notification, headphone buttons, audio focus, pausing
 * when the audio would become noisy, and the wake and Wi-Fi locks while it plays.
 */
package app.cratarr

import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

class PlayerService : MediaSessionService() {
    private var session: MediaSession? = null
    private lateinit var prefs: Prefs

    override fun onCreate() {
        super.onCreate()
        prefs = Prefs(this)
        val music = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        val player = ExoPlayer.Builder(this)
            .setAudioAttributes(music, true)
            .setHandleAudioBecomingNoisy(true)
            // A wake lock and a Wi-Fi lock while it plays: the screen off stops neither the music nor its download.
            .setWakeMode(C.WAKE_MODE_NETWORK)
            // Previous restarts the track after this long in, as the page's player does (Finamp: 5 s).
            .setMaxSeekToPreviousPositionMs(RESTART_AFTER_MS)
            .build()
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        session = MediaSession.Builder(this, player)
            .setSessionActivity(open)
            .setCallback(Callback())
            .build()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    /** Stay ready: the service keeps the foreground while paused too, so Android does not put the app to sleep. */
    override fun onUpdateNotification(session: MediaSession, startInForegroundRequired: Boolean) {
        val stay = prefs.stayReady && session.player.mediaItemCount > 0
        super.onUpdateNotification(session, startInForegroundRequired || stay)
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        // The app swiped away: music that plays goes on; a stopped player goes with the app.
        val player = session?.player
        if (player == null || !player.playWhenReady || player.mediaItemCount == 0) stopSelf()
    }

    override fun onDestroy() {
        session?.run {
            player.release()
            release()
        }
        session = null
        super.onDestroy()
    }

    /** Tracks arrive by their id only (`source/id`, each part URL-encoded); the file and the cover come from the server. */
    private inner class Callback : MediaSession.Callback {
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> = Futures.immediateFuture(mediaItems.map(::located).toMutableList())
    }

    private fun located(item: MediaItem): MediaItem {
        val server = prefs.serverUrl?.trimEnd('/') ?: return item
        return item.buildUpon()
            .setUri("$server/api/file/${item.mediaId}")
            .setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(Uri.parse("$server/api/cover/${item.mediaId}")).build())
            .build()
    }

    private companion object {
        const val RESTART_AFTER_MS = 5_000L
    }
}
