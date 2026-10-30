/*
 * Stay ready (decision 0031, amended 2026-10-19): a device that waits for Play On holds a foreground service of its own,
 * with the "Ready" notification, apart from the player's. Android lets an app start music only while it is on the
 * screen or holds a foreground service (from Android 15 on, its request is otherwise ignored and the player pauses at
 * once), and Media3 takes the player's service in and out of the foreground as the music plays and stops, at times on
 * an update of its own that the app does not see. The salon tablet, ready and idle, so took a handover and paused
 * (2026-10-19). Ready asks for it again whenever the app starts, comes to the front, connects, or finds it gone.
 */
package app.cratarr

import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat

class ReadyService : Service() {
    private val main = Handler(Looper.getMainLooper())

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // In the foreground first: a start asked for so must come there, also when it is let go at once.
        try {
            startForeground(Ready.NOTIFICATION, Ready.notification(this), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
            Ready.went(this, null, intent?.getStringExtra(Ready.EXTRA_WHY))
        } catch (e: IllegalStateException) {
            Ready.went(this, e, intent?.getStringExtra(Ready.EXTRA_WHY))
            stopSelf()
            return START_NOT_STICKY
        }
        main.removeCallbacksAndMessages(null)
        when {
            Prefs(this).stayReady -> {}
            // Not staying ready: in the foreground only for a moment, for a start of the music (the player's own
            // service holds it once the music plays).
            intent?.getBooleanExtra(Ready.EXTRA_BRIEFLY, false) == true -> main.postDelayed(::letGo, Ready.BRIEFLY_MS)
            else -> letGo()
        }
        return START_STICKY
    }

    private fun letGo() {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        super.onDestroy()
    }
}

object Ready {
    private const val TAG = "CratarrReady"
    const val NOTIFICATION = 7
    private const val CHANNEL = "ready"
    const val EXTRA_WHY = "why"
    const val EXTRA_BRIEFLY = "briefly"
    /** How long a device that does not stay ready holds the foreground for a start of the music. */
    const val BRIEFLY_MS = 60_000L

    /** The last start that failed (its error), for the log: each different failure is written, and the recovery. */
    @Volatile private var failing: String? = null

    /**
     * Ready asked for: with Stay ready, in the foreground (again; a start where it is already there changes nothing);
     * [briefly], for a start of the music, also without. Without either, the Ready service is let go. [why]: what asked,
     * for the log.
     */
    fun ensure(context: Context, why: String, briefly: Boolean = false) {
        val app = context.applicationContext
        val intent = Intent(app, ReadyService::class.java).putExtra(EXTRA_WHY, why).putExtra(EXTRA_BRIEFLY, briefly)
        if (!Prefs(app).stayReady && !briefly) {
            app.stopService(intent)
            return
        }
        try {
            ContextCompat.startForegroundService(app, intent)
        } catch (e: IllegalStateException) {
            // Android refuses a foreground start from the background, unless an exemption holds (battery optimisation
            // off for the app, an update just installed…); the next start from the front shows it.
            went(app, e, why)
        }
    }

    /** Whether one of the app's services is in the foreground now: Android lets it start music then. */
    @Suppress("DEPRECATION") // still answers for the app's own services
    fun foreground(context: Context): Boolean =
        context.getSystemService(ActivityManager::class.java)?.getRunningServices(Int.MAX_VALUE).orEmpty()
            .any { it.foreground && it.service.packageName == context.packageName }

    /** A start of the Ready service came out so ([error]: refused), written down as it changes. */
    fun went(context: Context, error: Exception?, why: String?) {
        val now = error?.let { it.message ?: it.javaClass.simpleName }
        if (now == failing) return
        val before = failing
        failing = now
        if (now != null) {
            Log.w(TAG, "cannot stay ready ($why): $now")
            LinkLog.add(context, "warn", "playon", "cannot stay ready", "why" to why, "error" to now)
        } else if (before != null) {
            LinkLog.add(context, "info", "playon", "ready again", "why" to why)
        }
    }

    fun notification(context: Context): Notification {
        val notes = context.getSystemService(NotificationManager::class.java)
        notes.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.ready), NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(context.getString(R.string.ready))
            .setContentText(Prefs(context).playerName)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
    }
}

/**
 * After the app is updated, or the device started: a device that stays ready connects again by itself, with no one to
 * open the app (an update stops it; Play On then waited for someone to, 2026-10-19).
 */
class ReadyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (!Prefs(context).stayReady) return
        LinkLog.add(context, "info", "playon", "starting by itself", "after" to intent.action?.substringAfterLast('.'))
        try {
            context.startService(Intent(context, PlayerService::class.java))
        } catch (e: IllegalStateException) {
            LinkLog.add(context, "warn", "playon", "cannot start by itself", "error" to (e.message ?: e.javaClass.simpleName))
        }
        Ready.ensure(context, intent.action?.substringAfterLast('.') ?: "started")
    }
}
