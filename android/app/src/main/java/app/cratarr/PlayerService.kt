/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's RemotePlayerService (jellyfin-android, GPL-2.0). The page reports
 * what plays; this service shows it on the lock screen and in the notification, and passes their buttons back to the
 * page. New here (decision 0031): wake and Wi-Fi locks while playing, and while idle when Stay ready; the foreground
 * notification before the cover; audio focus; pausing when the audio would become noisy; staying in the foreground
 * between tracks and while paused.
 */
package app.cratarr

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.wifi.WifiManager
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONException
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

class PlayerService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var prefs: Prefs
    private lateinit var notifications: NotificationManager
    private lateinit var audio: AudioManager
    private lateinit var wakeLock: PowerManager.WakeLock
    private lateinit var wifiLock: WifiManager.WifiLock
    private lateinit var session: MediaSession

    /** What plays, as the page last said; null when nothing does. */
    private var now: Playing? = null
    private var shownId: String? = null
    private var cover: Bitmap? = null
    private var coverUrl: String? = null
    private var hasFocus = false
    private var resumeOnFocus = false

    private data class Playing(
        val id: String,
        val title: String,
        val artist: String,
        val album: String,
        val cover: String,
        val position: Long,
        val duration: Long,
        val paused: Boolean,
        val canSeek: Boolean,
    )

    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            // Headphones unplugged, a Bluetooth speaker gone: the music would go on out loud.
            if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) PageCommands.send("pause")
        }
    }

    private val musicAttributes: AudioAttributes = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
        .build()

    private val focusRequest: AudioFocusRequest by lazy {
        AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(musicAttributes)
            .setOnAudioFocusChangeListener(::onFocusChange)
            .build()
    }

    override fun onCreate() {
        super.onCreate()
        prefs = Prefs(this)
        notifications = getSystemService(NotificationManager::class.java)
        audio = getSystemService(AudioManager::class.java)
        wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cratarr:player")
            .apply { setReferenceCounted(false) }
        @Suppress("DEPRECATION") // the mode that keeps Wi-Fi up with the screen off
        wifiLock = (applicationContext.getSystemService(WIFI_SERVICE) as WifiManager)
            .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "cratarr:player")
            .apply { setReferenceCounted(false) }
        ContextCompat.registerReceiver(
            this,
            noisy,
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        notifications.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.playback), NotificationManager.IMPORTANCE_LOW),
        )
        session = MediaSession(this, "Cratarr").apply {
            setCallback(callback)
            setPlaybackToLocal(musicAttributes)
        }
    }

    override fun onBind(intent: Intent): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STATE -> now = parse(intent.getStringExtra(EXTRA_STATE)) ?: now
            ACTION_STOPPED -> now = null
            ACTION_QUIT -> {
                now = null
                leave()
                return START_NOT_STICKY
            }
            ACTION_PLAY -> PageCommands.send("play")
            ACTION_PAUSE -> PageCommands.send("pause")
            ACTION_NEXT -> PageCommands.send("next")
            ACTION_PREVIOUS -> PageCommands.send("previous")
            ACTION_STOP -> PageCommands.send("stop")
        }
        update()
        // Not restarted on its own: without the page in the activity there is nothing to keep alive.
        return START_NOT_STICKY
    }

    /** Show what plays (or that the app is ready), hold what it needs, and leave when there is nothing to do. */
    private fun update() {
        val playing = now
        // Foreground first, every time: Android stops an app whose foreground service does not show itself in time.
        startForeground(NOTIFICATION, notification(playing), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        inForeground = true
        if (playing == null && !prefs.stayReady) {
            leave()
            return
        }
        if (playing != null) {
            if (playing.id != shownId) {
                shownId = playing.id
                setMetadata(playing)
            }
            if (playing.cover != coverUrl) loadCover(playing.cover)
            setState(playing)
        }
        session.isActive = playing != null
        hold(playing != null && !playing.paused || prefs.stayReady)
        focus(playing != null)
    }

    private fun leave() {
        session.isActive = false
        hold(false)
        focus(false)
        stopForeground(STOP_FOREGROUND_REMOVE)
        inForeground = false
        stopSelf()
    }

    private fun parse(json: String?): Playing? = try {
        val o = JSONObject(json ?: return null)
        Playing(
            id = o.optString("id"),
            title = o.optString("title"),
            artist = o.optString("artist"),
            album = o.optString("album"),
            cover = o.optString("cover"),
            position = o.optLong("position", PlaybackState.PLAYBACK_POSITION_UNKNOWN),
            duration = o.optLong("duration", 0),
            paused = o.optBoolean("paused", true),
            canSeek = o.optBoolean("canSeek", true),
        )
    } catch (e: JSONException) {
        Log.w(TAG, "a state the page sent cannot be read: ${e.message}")
        null
    }

    // The wake lock is held without a timeout on purpose: for as long as music plays, or the device stays ready.
    @SuppressLint("WakelockTimeout")
    private fun hold(on: Boolean) {
        if (on) {
            if (!wakeLock.isHeld) wakeLock.acquire()
            if (!wifiLock.isHeld) wifiLock.acquire()
        } else {
            if (wakeLock.isHeld) wakeLock.release()
            if (wifiLock.isHeld) wifiLock.release()
        }
    }

    private fun focus(on: Boolean) {
        if (on && !hasFocus) {
            hasFocus = audio.requestAudioFocus(focusRequest) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        } else if (!on && hasFocus) {
            audio.abandonAudioFocusRequest(focusRequest)
            hasFocus = false
            resumeOnFocus = false
        }
    }

    private fun onFocusChange(change: Int) {
        val playing = now
        when (change) {
            AudioManager.AUDIOFOCUS_LOSS -> {
                // Another app plays now: ours pauses, and is not resumed by itself.
                hasFocus = false
                resumeOnFocus = false
                if (playing != null && !playing.paused) PageCommands.send("pause")
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                // A call, a navigation prompt: pause, and play on when it is over.
                if (playing != null && !playing.paused) {
                    resumeOnFocus = true
                    PageCommands.send("pause")
                }
            }
            AudioManager.AUDIOFOCUS_GAIN -> {
                hasFocus = true
                if (resumeOnFocus) PageCommands.send("play")
                resumeOnFocus = false
            }
            // AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK: Android lowers the volume itself.
        }
    }

    private fun setMetadata(playing: Playing) {
        session.setMetadata(
            MediaMetadata.Builder().apply {
                putString(MediaMetadata.METADATA_KEY_MEDIA_ID, playing.id)
                putString(MediaMetadata.METADATA_KEY_TITLE, playing.title)
                putString(MediaMetadata.METADATA_KEY_ARTIST, playing.artist)
                putString(MediaMetadata.METADATA_KEY_ALBUM, playing.album)
                putLong(MediaMetadata.METADATA_KEY_DURATION, playing.duration)
                cover?.takeIf { coverUrl == playing.cover }?.let { putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, it) }
            }.build(),
        )
    }

    private fun setState(playing: Playing) {
        val actions = ACTIONS or (if (playing.canSeek) PlaybackState.ACTION_SEEK_TO else 0L)
        val state = if (playing.paused) PlaybackState.STATE_PAUSED else PlaybackState.STATE_PLAYING
        session.setPlaybackState(
            PlaybackState.Builder()
                .setActions(actions)
                .setState(state, playing.position, if (playing.paused) 0f else 1f)
                .build(),
        )
    }

    /** The cover comes after the notification (a slow server must not hold up the foreground start). */
    private fun loadCover(url: String) {
        coverUrl = url
        cover = null
        if (url.isEmpty()) return
        scope.launch {
            val bitmap = withContext(Dispatchers.IO) { fetchBitmap(url) }
            val playing = now
            if (bitmap == null || coverUrl != url || playing == null) return@launch
            cover = bitmap
            setMetadata(playing)
            notifications.notify(NOTIFICATION, notification(playing))
        }
    }

    private fun fetchBitmap(url: String): Bitmap? = try {
        val absolute = URL(URL(prefs.serverUrl ?: url), url)
        val connection = absolute.openConnection() as HttpURLConnection
        connection.connectTimeout = 10_000
        connection.readTimeout = 10_000
        connection.inputStream.use { BitmapFactory.decodeStream(it) }?.let { scaled(it) }
    } catch (e: Exception) {
        Log.w(TAG, "no cover from $url: ${e.message}")
        null
    }

    private fun scaled(b: Bitmap): Bitmap {
        val longest = maxOf(b.width, b.height)
        if (longest <= COVER_SIZE) return b
        val f = COVER_SIZE.toFloat() / longest
        return Bitmap.createScaledBitmap(b, (b.width * f).toInt(), (b.height * f).toInt(), true)
    }

    private fun notification(playing: Playing?): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(open)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
        if (playing == null) {
            return builder.setContentTitle(getString(R.string.ready)).setOngoing(true).build()
        }
        return builder
            .setContentTitle(playing.title)
            .setContentText(playing.artist)
            .setSubText(playing.album)
            .setLargeIcon(cover?.takeIf { coverUrl == playing.cover })
            .setOngoing(!playing.paused)
            .setDeleteIntent(serviceIntent(ACTION_STOP))
            .addAction(action(R.drawable.ic_skip_previous_black_32dp, R.string.action_previous, ACTION_PREVIOUS))
            .addAction(
                if (playing.paused) {
                    action(R.drawable.ic_play_black_42dp, R.string.action_play, ACTION_PLAY)
                } else {
                    action(R.drawable.ic_pause_black_42dp, R.string.action_pause, ACTION_PAUSE)
                },
            )
            .addAction(action(R.drawable.ic_skip_next_black_32dp, R.string.action_next, ACTION_NEXT))
            .addAction(action(R.drawable.ic_stop_black_32dp, R.string.action_stop, ACTION_STOP))
            .setStyle(Notification.MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))
            .build()
    }

    private fun serviceIntent(action: String): PendingIntent = PendingIntent.getService(
        this,
        action.hashCode(),
        Intent(this, PlayerService::class.java).setAction(action),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun action(@DrawableRes icon: Int, @StringRes title: Int, action: String): Notification.Action =
        Notification.Action.Builder(android.graphics.drawable.Icon.createWithResource(this, icon), getString(title), serviceIntent(action)).build()

    private val callback = object : MediaSession.Callback() {
        override fun onPlay() = PageCommands.send("play")

        override fun onPause() = PageCommands.send("pause")

        override fun onSkipToNext() = PageCommands.send("next")

        override fun onSkipToPrevious() = PageCommands.send("previous")

        override fun onStop() = PageCommands.send("stop")

        override fun onSeekTo(pos: Long) {
            PageCommands.send("seek", pos)
            // Shown at once; the page's next report confirms it.
            now = now?.copy(position = pos)?.also { setState(it) }
        }
        // Media buttons (headset, Bluetooth) arrive here through MediaSession's own handling: play/pause, next, previous.
    }

    override fun onDestroy() {
        unregisterReceiver(noisy)
        hold(false)
        focus(false)
        scope.cancel()
        session.release()
        inForeground = false
        super.onDestroy()
    }

    companion object {
        private const val TAG = "CratarrPlayer"
        private const val CHANNEL = "playback"
        private const val NOTIFICATION = 1
        private const val COVER_SIZE = 512
        private const val ACTIONS = PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_PLAY or
            PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_STOP or PlaybackState.ACTION_SKIP_TO_NEXT or
            PlaybackState.ACTION_SKIP_TO_PREVIOUS

        private const val ACTION_STATE = "cratarr.STATE"
        private const val ACTION_STOPPED = "cratarr.STOPPED"
        private const val ACTION_READY = "cratarr.READY"
        private const val ACTION_QUIT = "cratarr.QUIT"
        private const val ACTION_PLAY = "cratarr.PLAY"
        private const val ACTION_PAUSE = "cratarr.PAUSE"
        private const val ACTION_NEXT = "cratarr.NEXT"
        private const val ACTION_PREVIOUS = "cratarr.PREVIOUS"
        private const val ACTION_STOP = "cratarr.STOP"
        private const val EXTRA_STATE = "state"

        /** Whether the service is in the foreground now: then it may be told things from the background too. */
        @Volatile
        var inForeground = false
            private set

        /** What plays now, as the page reports it (JSON: id, title, artist, album, cover, position, duration, paused). */
        fun report(context: Context, json: String) = send(context, Intent(context, PlayerService::class.java).setAction(ACTION_STATE).putExtra(EXTRA_STATE, json))

        /** Nothing plays any more. */
        fun stopped(context: Context) {
            if (inForeground) send(context, Intent(context, PlayerService::class.java).setAction(ACTION_STOPPED))
        }

        /** Started (or left) as Stay ready says. Called while the app is in front, where a foreground start is allowed. */
        fun sync(context: Context) {
            if (Prefs(context).stayReady || inForeground) send(context, Intent(context, PlayerService::class.java).setAction(ACTION_READY))
        }

        /** The app is closed: the page is gone, so is the player. */
        fun quit(context: Context) {
            if (inForeground) context.startService(Intent(context, PlayerService::class.java).setAction(ACTION_QUIT))
        }

        private fun send(context: Context, intent: Intent) {
            try {
                // Once in the foreground the app counts as in front, and a plain start reaches the service.
                if (inForeground) context.startService(intent) else ContextCompat.startForegroundService(context, intent)
            } catch (e: IllegalStateException) {
                // Android refuses a foreground start from the background (the phone, locked, not Stay ready).
                Log.w(TAG, "cannot start the player from the background: ${e.message}")
            }
        }
    }
}
