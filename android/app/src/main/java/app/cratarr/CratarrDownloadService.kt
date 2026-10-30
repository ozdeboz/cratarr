/*
 * Cratarr's own (decision 0033, step 4): Media3's download service for the owner's downloads, with its progress
 * notification; Android's job scheduler starts it again when Wi-Fi (or a network, as the settings say) comes back.
 */
package app.cratarr

import android.app.Notification
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.offline.DownloadNotificationHelper
import androidx.media3.exoplayer.offline.DownloadService
import androidx.media3.exoplayer.scheduler.PlatformScheduler
import androidx.media3.exoplayer.scheduler.Scheduler

@OptIn(UnstableApi::class)
class CratarrDownloadService : DownloadService(
    NOTIFICATION,
    DEFAULT_FOREGROUND_NOTIFICATION_UPDATE_INTERVAL,
    CHANNEL,
    R.string.downloads,
    0,
) {
    private val notifications by lazy { DownloadNotificationHelper(this, CHANNEL) }

    override fun getDownloadManager(): DownloadManager = Downloads.manager(this)

    override fun getScheduler(): Scheduler = PlatformScheduler(this, JOB)

    override fun getForegroundNotification(downloads: MutableList<Download>, notMetRequirements: Int): Notification =
        notifications.buildProgressNotification(this, R.drawable.media3_notification_small_icon, null, null, downloads, notMetRequirements)

    companion object {
        private const val NOTIFICATION = 8
        /** The downloads' notification channel; an export being saved (ExportSave) shows on it too. */
        const val CHANNEL = "downloads"
        private const val JOB = 1
    }
}
