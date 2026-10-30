/*
 * "Download… as files" (0.9.19): an export Cratarr prepared (`/api/export/{token}`) written file by file into a folder
 * the user picked with Android's folder picker, its subfolders as the export names them. A file there of the same name
 * is replaced, so an export run again overwrites. The page hears the progress over the bridge; Android shows it in a
 * notification. The writing runs on a thread of its own, the export service holding the app in the foreground
 * meanwhile, so it goes on when the page is left.
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
import android.provider.DocumentsContract
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.core.content.ContextCompat
import androidx.documentfile.provider.DocumentFile
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/** The parts of an export that need no Android: its paths, file types and addresses. */
object ExportPaths {
    /** What a file is created as when its extension says nothing: the folder's provider then keeps its name as is. */
    const val ANY_TYPE = "application/octet-stream"

    /**
     * A path of the export's (`/` between folders) as its folders and its file's name; null for one that would leave
     * the picked folder or names nothing: empty, absolute, `.` or `..` anywhere, an empty part, a backslash.
     */
    fun parts(path: String): Pair<List<String>, String>? {
        if (path.isEmpty() || path.startsWith("/")) return null
        val names = path.split('/')
        if (names.any { it.isEmpty() || it == "." || it == ".." || '\\' in it || '\u0000' in it }) return null
        return names.dropLast(1) to names.last()
    }

    /** A file name's extension, lower case; empty without one. */
    fun extension(name: String): String = name.substringAfterLast('.', "").lowercase(Locale.ROOT)

    /** A file's type by its extension. */
    fun typeOf(name: String): String = when (extension(name)) {
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
     * The type [name] is created with. Android's own folders put the extension of the type they are given on a name
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

    /** A file of the export's from Cratarr: `{server}/api/export/{token}/file?path=…`, `/` encoded too (Cratarr decodes
     *  the query). */
    fun fileUrl(server: String, token: String, path: String): String =
        "${server.trimEnd('/')}/api/export/${encode(token)}/file?path=${encode(path)}"

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

object ExportFiles {
    private const val TAG = "CratarrExport"
    const val NOTIFICATION = 11
    private const val DONE_NOTIFICATION = 12
    private const val CONNECT_MS = 15_000
    /** Cratarr converts a file before it sends it: its first byte can be a while coming. */
    private const val READ_MS = 120_000

    /** One export: the folder picked, Cratarr's token and download name, and its files' paths in order. */
    class Job(val tree: Uri, val token: String, val name: String, val paths: List<String>)

    /** A file not written: why, and whether Cratarr no longer knows the export (404). */
    private class Failure(val error: String, val gone: Boolean = false)

    private val worker = Executors.newSingleThreadExecutor { Thread(it, "cratarr-export").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())
    /** Exports asked for and not yet finished. */
    private val pending = AtomicInteger()

    /** The export service while it runs (the main thread's). */
    var service: ExportService? = null

    /** Told each message for the page (MainActivity sends it on), from the export's thread. */
    @Volatile var onReport: ((String) -> Unit)? = null

    /** The running export's last progress, for a page that says hello meanwhile; null once it ends. */
    @Volatile var latest: String? = null
        private set

    val busy: Boolean get() = pending.get() > 0

    /** Where the folder picker opens: the folder picked last, while the app may still write there. */
    fun startAt(context: Context): Uri? {
        val tree = Prefs(context).exportFolder?.let(Uri::parse) ?: return null
        val kept = context.contentResolver.persistedUriPermissions.any { it.uri == tree && it.isWritePermission }
        if (!kept) return null
        return try {
            DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        } catch (e: IllegalArgumentException) {
            null
        }
    }

    /** The folder picked: the app may write there from now on (also after a restart), and the picker opens there next
     *  time. The one picked before is let go, so the app holds only the last. */
    fun keepFolder(context: Context, tree: Uri) {
        val resolver = context.contentResolver
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        try {
            resolver.takePersistableUriPermission(tree, flags)
        } catch (e: SecurityException) {
            // Not offered for keeps: this export still writes there.
            Log.w(TAG, "cannot keep $tree: ${e.message}")
        }
        val prefs = Prefs(context)
        prefs.exportFolder?.let(Uri::parse)?.takeIf { it != tree }?.let { before ->
            try {
                resolver.releasePersistableUriPermission(before, flags)
            } catch (e: SecurityException) {
                // already let go
            }
        }
        prefs.exportFolder = tree.toString()
    }

    /** The page's `export-files` message as a job into [tree]; null without a token or files. */
    fun jobOf(tree: Uri, message: JSONObject): Job? {
        val token = message.optString("token")
        val files = message.optJSONArray("files") ?: return null
        val paths = (0 until files.length()).map { files.optJSONObject(it)?.optString("path").orEmpty() }
        if (token.isEmpty() || paths.isEmpty()) return null
        return Job(tree, token, message.optString("name"), paths)
    }

    /** Writes [job] after any export before it, the export service in the foreground until the last one ends. */
    fun start(context: Context, job: Job) {
        val app = context.applicationContext
        pending.incrementAndGet()
        try {
            ContextCompat.startForegroundService(app, Intent(app, ExportService::class.java))
        } catch (e: IllegalStateException) {
            // Android refused the foreground (the app not in front): the export runs anyway, while the app does.
            Log.w(TAG, "export service not started: ${e.message}")
        }
        worker.execute {
            try {
                write(app, job)
            } catch (e: Exception) {
                Log.w(TAG, "export ${job.token}", e)
                report(message(job, "failed").put("error", e.message ?: e.javaClass.simpleName))
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

    private fun message(job: Job, state: String) =
        JSONObject().put("type", "export-files").put("state", state).put("token", job.token)

    private fun report(m: JSONObject) {
        onReport?.invoke(m.toString())
    }

    private fun write(app: Context, job: Job) {
        val server = Prefs(app).serverUrl?.trimEnd('/')
        val root = DocumentFile.fromTreeUri(app, job.tree)
        if (server == null || root == null) {
            val error = if (server == null) "No address of Cratarr's is set." else "That folder cannot be written to."
            report(message(job, "failed").put("error", error))
            notifyDone(app, job, error)
            return
        }
        val folders = Folders(root)
        val total = job.paths.size
        val failed = JSONArray()
        var written = 0
        for ((i, path) in job.paths.withIndex()) {
            val progress = message(job, "progress").put("done", i).put("total", total).put("path", path).toString()
            latest = progress
            onReport?.invoke(progress)
            notifyProgress(app, job, i, total)
            val failure = try {
                writeOne(app, server, job, path, folders)
            } catch (e: Exception) {
                Failure(e.message ?: e.javaClass.simpleName)
            }
            when {
                failure == null -> written++
                // Gone at the first file: the export's token expired before the folder was picked.
                failure.gone && i == 0 -> {
                    val error = app.getString(R.string.export_expired)
                    report(message(job, "failed").put("error", error))
                    notifyDone(app, job, error)
                    return
                }
                else -> failed.put(JSONObject().put("path", path).put("error", failure.error))
            }
        }
        report(message(job, "done").put("written", written).put("failed", failed))
        notifyDone(
            app, job,
            if (failed.length() == 0) app.getString(R.string.export_written, written) else app.getString(R.string.export_written_failed, written, failed.length()),
        )
    }

    /** One file: asked of Cratarr first, so a refusal leaves a file already there as it is; then written over it. */
    private fun writeOne(app: Context, server: String, job: Job, path: String, folders: Folders): Failure? {
        val (dirs, name) = ExportPaths.parts(path) ?: return Failure("Not a path inside the folder.")
        val c = Net.open(app, ExportPaths.fileUrl(server, job.token, path))
        c.connectTimeout = CONNECT_MS
        c.readTimeout = READ_MS
        try {
            val code = Net.code(app, c)
            if (code !in 200..299) return Failure(errorOf(c, code), gone = code == 404)
            val dir = folders.dir(dirs) ?: return Failure("The folder could not be made.")
            val type = ExportPaths.createType(name, MimeTypeMap.getSingleton()::getMimeTypeFromExtension, MimeTypeMap.getSingleton()::getExtensionFromMimeType)
            val file = folders.file(dir, name, type) ?: return Failure("The file could not be made.")
            try {
                val out = app.contentResolver.openOutputStream(file.uri) ?: return Failure("The file could not be opened.")
                out.use { o -> c.inputStream.use { it.copyTo(o, 64 * 1024) } }
            } catch (e: Exception) {
                // Half a file is no file.
                file.delete()
                throw e
            }
            return null
        } finally {
            c.disconnect()
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

    /**
     * The picked folder's subfolders as one export goes through them: each folder's names read once (a lookup by name
     * in Android's folders is a query per entry), made where missing.
     */
    private class Folders(private val root: DocumentFile) {
        private val dirs = HashMap<String, DocumentFile>()
        private val names = HashMap<Uri, HashMap<String, DocumentFile>>()

        private fun namesIn(dir: DocumentFile): HashMap<String, DocumentFile> = names.getOrPut(dir.uri) {
            HashMap<String, DocumentFile>().also { m -> for (f in dir.listFiles()) f.name?.let { m[it] = f } }
        }

        /** The folder at [path] under the picked one; null when it cannot be made (a file in the way). */
        fun dir(path: List<String>): DocumentFile? {
            var at = root
            for (i in path.indices) {
                val key = path.subList(0, i + 1).joinToString("/")
                val parent = at
                at = dirs[key] ?: run {
                    val there = namesIn(parent)
                    val found = there[path[i]]
                    val made = when {
                        found == null -> parent.createDirectory(path[i])?.also { there[path[i]] = it }
                        found.isDirectory -> found
                        else -> null
                    } ?: return null
                    made.also { dirs[key] = it }
                }
            }
            return at
        }

        /** A new file [name] in [dir]: one there of that name is deleted first (a folder of that name stays: null). */
        fun file(dir: DocumentFile, name: String, type: String): DocumentFile? {
            val there = namesIn(dir)
            there[name]?.let { old ->
                if (!old.isFile || !old.delete()) return null
                there.remove(name)
            }
            return dir.createFile(type, name)?.also { there[name] = it }
        }
    }

    // --- the notification, on the downloads' channel ---

    private fun channel(context: Context): NotificationManager? {
        val notes = context.getSystemService(NotificationManager::class.java) ?: return null
        notes.createNotificationChannel(NotificationChannel(CratarrDownloadService.CHANNEL, context.getString(R.string.downloads), NotificationManager.IMPORTANCE_LOW))
        return notes
    }

    private fun titleOf(context: Context, job: Job?) =
        job?.name?.removeSuffix(".zip")?.takeIf { it.isNotBlank() } ?: context.getString(R.string.downloads)

    private fun builder(context: Context, job: Job?): Notification.Builder {
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(context, CratarrDownloadService.CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(titleOf(context, job))
            .setContentIntent(open)
    }

    /** The export service's own notification as it starts (the progress one takes its place). */
    fun notification(context: Context): Notification {
        channel(context)
        return builder(context, null).setOngoing(true).setProgress(0, 0, true).build()
    }

    private fun notifyProgress(context: Context, job: Job, done: Int, total: Int) {
        val note = builder(context, job)
            .setContentText(context.getString(R.string.export_progress, done + 1, total))
            .setProgress(total, done, false)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
        post(context, NOTIFICATION, note)
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

/** Holds the app in the foreground while an export is written (ExportFiles); lets go once the last one ends. */
class ExportService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            startForeground(ExportFiles.NOTIFICATION, ExportFiles.notification(this), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } catch (e: Exception) {
            // Refused (Android 15's daily time for data sync used up, say): the export goes on without.
            Log.w("CratarrExport", "not in the foreground: ${e.message}")
            stopSelf()
            return START_NOT_STICKY
        }
        ExportFiles.service = this
        if (!ExportFiles.busy) finish()
        return START_NOT_STICKY
    }

    fun finish() {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    /** Android 15's limit for data sync reached: out of the foreground (the export goes on while the app runs). */
    override fun onTimeout(startId: Int, fgsType: Int) = finish()

    override fun onDestroy() {
        if (ExportFiles.service === this) ExportFiles.service = null
        super.onDestroy()
    }
}
