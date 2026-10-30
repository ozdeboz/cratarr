package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent

/**
 * One short line from Cratarr (decision 0079: a queue request asked of this account, or one of its own answered), sent
 * as Play On's `notice`: a notification while the app is not in front; in front, the page says it itself.
 */
object Notices {
    /** The app's page is in front: between MainActivity's start and stop. */
    @Volatile
    var inFront = false

    private const val CHANNEL = "requests"
    private const val NOTIFICATION = 11

    fun show(context: Context, text: String) {
        if (inFront || text.isBlank()) return
        val notes = context.getSystemService(NotificationManager::class.java) ?: return
        notes.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.requests), NotificationManager.IMPORTANCE_DEFAULT))
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val note = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(text)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        try {
            notes.notify(NOTIFICATION, note)
        } catch (e: SecurityException) {
            // Notifications not allowed: the page shows it once the app is opened.
        }
    }
}
