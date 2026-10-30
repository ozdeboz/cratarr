/*
 * Download… in the app (0.10): an export Cratarr prepared (`/api/export/{token}`) saved as the one file it downloads
 * as, where the user says in Android's save dialog. Several files come as a zip, one file as that file (Cratarr
 * decides, and names it). The page hears the progress over the bridge; Android shows it in a notification. The saving
 * runs on a thread of its own, the export service holding the app in the foreground meanwhile, so it goes on when the
 * page is left.
 */
package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.provider.DocumentsContract
import android.text.format.Formatter
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/** The parts of a download that need no Android: its file's type and its address. */
object ExportPaths {
    /** What a file is saved as when its extension says nothing: the provider then keeps its name as is. */
    const val ANY_TYPE = "application/octet-stream"

    /** A file name's extension, lower case; empty without one. */
    fun extension(name: String): String = name.substringAfterLast('.', "").lowercase(Locale.ROOT)

    /** A file's type by its extension. */
    fun typeOf(name: String): String = when (extension(name)) {
        "zip" -> "application/zip"
        "mp3" -> "audio/mpeg"
        "flac" -> "audio/flac"
        "opus", "ogg" -> "audio/ogg"
        "m4a" -> "audio/mp4"
        "wav" -> "audio/wav"
        "aif", "aiff" -> "audio/aiff"
        "m3u" -> "audio/x-mpegurl"
        "txt" -> "text/plain"
        else -> ANY_TYPE
    }

    /**
     * The type [name] is saved with. Android's own folders put the extension of the type they are given on a name
     * whose extension does not go with it (`So What.opus.ogg`): so [typeOf] when the device's table agrees with it
     * ([typeOfExtension], [extensionOfType]: MimeTypeMap's), else the device's type for the extension, else [ANY_TYPE].
     */
    fun createType(name: String, typeOfExtension: (String) -> String?, extensionOfType: (String) -> String?): String {
        val ext = extension(name)
        if (ext.isEmpty()) return ANY_TYPE
        val ours = typeOf(name)
        val theirs = typeOfExtension(ext)
        if (theirs == ours || extensionOfType(ours) == ext) return ours
        return theirs ?: ANY_TYPE
    }

    /** The prepared export's download: `{server}/api/export/{token}`. */
    fun url(server: String, token: String): String = "${server.trimEnd('/')}/api/export/${encode(token)}"

    private const val HEX = "0123456789ABCDEF"

    /** Percent-encoded as Android's Uri.encode does it (which the JVM tests cannot call): UTF-8, everything but letters,
     *  digits and `_-!.~'()*`. */
    fun encode(s: String): String {
        val out = StringBuilder(s.length)
        for (b in s.toByteArray(Charsets.UTF_8)) {
            val c = b.toInt() and 0xff
            val ch = c.toChar()
            if (c < 0x80 && (ch in 'a'..'z' || ch in 'A'..'Z' || ch in '0'..'9' || ch in "_-!.~'()*")) {
                out.append(ch)
            } else {
                out.append('%').append(HEX[c shr 4]).append(HEX[c and 15])
            }
        }
        return out.toString()
    }
}

object ExportSave {
    private const val TAG = "CratarrExport"
    const val NOTIFICATION = 11
    private const val DONE_NOTIFICATION = 12
    private const val CONNECT_MS = 15_000
    /** Cratarr converts a file before it sends it (a file alone in full first): its next byte can be a while coming. */
    private const val READ_MS = 120_000
    /** The page and the notification hear the progress at most this often. */
    private const val PROGRESS_MS = 500L

    /** Android's save dialog for a download of this name, its type by the name's extension (one that did not go with
     *  the name would have the dialog add an extension of its own). The document made, or null when dismissed. */
    class SaveAs : ActivityResultContracts.CreateDocument(ExportPaths.ANY_TYPE) {
        override fun createIntent(context: Context, input: String): Intent {
            val map = MimeTypeMap.getSingleton()
            return super.createIntent(context, input).setType(ExportPaths.createType(input, map::getMimeTypeFromExtension, map::getExtensionFromMimeType))
        }
    }

    /** One download: the document the user made for it, Cratarr's token and the download's name. */
    class Job(val doc: Uri, val token: String, val name: String)

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "cratarr-export").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())
    /** Downloads asked for and not yet finished. */
    private val pending = AtomicInteger()

    /** The export service while it runs (the main thread's). */
    var service: ExportService? = null

    /** Told each message for the page (MainActivity sends it on), from the saving's thread. */
    @Volatile var onReport: ((String) -> Unit)? = null

    /** The running download's last progress, for a page that says hello meanwhile; null once it ends. */
    @Volatile var latest: String? = null
        private set

    val busy: Boolean get() = pending.get() > 0

    /** A message for the page about the download of [token]: `cancelled`, `progress`, `done` or `failed`. */
    fun message(token: String, state: String): JSONObject =
        JSONObject().put("type", "export-save").put("token", token).put("state", state)

    /** Saves [job] after any download before it, the export service in the foreground until the last one ends. */
    fun start(context: Context, job: Job) {
        val app = context.applicationContext
        pending.incrementAndGet()
        try {
            ContextCompat.startForegroundService(app, Intent(app, ExportService::class.java))
        } catch (e: IllegalStateException) {
            // Android refused the foreground (the app not in front): the download runs anyway, while the app does.
            Log.w(TAG, "export service not started: ${e.message}")
        }
        worker.execute {
            try {
                save(app, job)
            } finally {
                latest = null
                pending.decrementAndGet()
                main.post {
                    if (!busy) {
                        service?.finish()
                        app.getSystemService(NotificationManager::class.java)?.cancel(NOTIFICATION)
                    }
                }
            }
        }
    }

    private fun report(m: JSONObject) {
        onReport?.invoke(m.toString())
    }

    /** The download into the document, its bytes told as they come; one that fails leaves no half file behind. */
    private fun save(app: Context, job: Job) {
        val error = try {
            val bytes = fetch(app, job)
            report(message(job.token, "done").put("bytes", bytes))
            notifyDone(app, job, Formatter.formatShortFileSize(app, bytes))
            return
        } catch (e: Refused) {
            e.message.orEmpty()
        } catch (e: Exception) {
            Log.w(TAG, "export ${job.token}", e)
            e.message ?: e.javaClass.simpleName
        }
        delete(app, job.doc)
        report(message(job.token, "failed").put("error", error))
        notifyDone(app, job, error)
    }

    /** Cratarr said no (its words, or its status), or there is no address to ask. */
    private class Refused(message: String) : Exception(message)

    /** Streams the download into the document; its length in bytes. */
    private fun fetch(app: Context, job: Job): Long {
        val server = Prefs(app).serverUrl ?: throw Refused("No address of Cratarr's is set.")
        val c = Net.open(app, ExportPaths.url(server, job.token))
        c.connectTimeout = CONNECT_MS
        c.readTimeout = READ_MS
        try {
            val code = Net.code(app, c)
            if (code == 404) throw Refused(app.getString(R.string.export_expired))
            if (code !in 200..299) throw Refused(errorOf(c, code))
            val total = c.contentLengthLong.takeIf { it >= 0 }
            var bytes = 0L
            var told = 0L
            progress(app, job, bytes, total)
            val out = app.contentResolver.openOutputStream(job.doc, "w") ?: throw IOException("The file could not be opened.")
            out.use { o ->
                c.inputStream.use { input ->
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        o.write(buf, 0, n)
                        bytes += n
                        val now = SystemClock.elapsedRealtime()
                        if (now - told >= PROGRESS_MS) {
                            told = now
                            progress(app, job, bytes, total)
                        }
                    }
                }
            }
            if (total != null && bytes != total) throw IOException("The download broke off.")
            return bytes
        } finally {
            c.disconnect()
        }
    }

    /** Half a file is no file: the document goes. A provider that cannot delete keeps it (logged). */
    private fun delete(app: Context, doc: Uri) {
        try {
            if (!DocumentsContract.deleteDocument(app.contentResolver, doc)) Log.w(TAG, "not deleted: $doc")
        } catch (e: Exception) {
            Log.w(TAG, "not deleted: $doc: ${e.message}")
        }
    }

    /** Cratarr's own words for a refusal (`{"error": …}`), else its status. */
    private fun errorOf(c: HttpURLConnection, code: Int): String {
        val said = try {
            c.errorStream?.use { JSONObject(it.readBytes().toString(Charsets.UTF_8)).optString("error") }
        } catch (e: Exception) {
            null
        }
        return said?.takeIf { it.isNotBlank() } ?: "Cratarr answered $code."
    }

    private fun progress(app: Context, job: Job, bytes: Long, total: Long?) {
        val m = message(job.token, "progress").put("bytes", bytes)
        if (total != null) m.put("total", total)
        val said = m.toString()
        latest = said
        onReport?.invoke(said)
        notifyProgress(app, job, bytes, total)
    }

    // --- the notification, on the downloads' channel ---

    private fun channel(context: Context): NotificationManager? {
        val notes = context.getSystemService(NotificationManager::class.java) ?: return null
        notes.createNotificationChannel(NotificationChannel(CratarrDownloadService.CHANNEL, context.getString(R.string.downloads), NotificationManager.IMPORTANCE_LOW))
        return notes
    }

    private fun builder(context: Context, job: Job?): Notification.Builder {
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(context, CratarrDownloadService.CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(job?.name?.takeIf { it.isNotBlank() } ?: context.getString(R.string.downloads))
            .setContentIntent(open)
    }

    /** The export service's own notification as it starts (the progress one takes its place). */
    fun notification(context: Context): Notification {
        channel(context)
        return builder(context, null).setOngoing(true).setProgress(0, 0, true).build()
    }

    /** In bytes when the length is known (a file as it is, or a zip of files as they are), else how much so far. */
    private fun notifyProgress(context: Context, job: Job, bytes: Long, total: Long?) {
        val so = Formatter.formatShortFileSize(context, bytes)
        val note = builder(context, job)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
        if (total != null && total > 0) {
            note.setContentText(context.getString(R.string.export_progress, so, Formatter.formatShortFileSize(context, total)))
                .setProgress(1000, (bytes * 1000 / total).toInt(), false)
        } else {
            note.setContentText(so).setProgress(0, 0, true)
        }
        post(context, NOTIFICATION, note.build())
    }

    private fun notifyDone(context: Context, job: Job, text: String) {
        post(context, DONE_NOTIFICATION, builder(context, job).setContentText(text).setAutoCancel(true).build())
    }

    private fun post(context: Context, id: Int, note: Notification) {
        try {
            channel(context)?.notify(id, note)
        } catch (e: SecurityException) {
            // Notifications not allowed: the page still hears it.
        }
    }
}

/** Holds the app in the foreground while a download is saved (ExportSave); lets go once the last one ends. */
class ExportService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            startForeground(ExportSave.NOTIFICATION, ExportSave.notification(this), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } catch (e: Exception) {
            // Refused (Android 15's daily time for data sync used up, say): the download goes on without.
            Log.w("CratarrExport", "not in the foreground: ${e.message}")
            stopSelf()
            return START_NOT_STICKY
        }
        ExportSave.service = this
        if (!ExportSave.busy) finish()
        return START_NOT_STICKY
    }

    fun finish() {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    /** Android 15's limit for data sync reached: out of the foreground (the download goes on while the app runs). */
    override fun onTimeout(startId: Int, fgsType: Int) = finish()

    override fun onDestroy() {
        if (ExportSave.service === this) ExportSave.service = null
        super.onDestroy()
    }
}
