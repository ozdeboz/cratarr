package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/**
 * The app's own log of its link to Cratarr and of its player service (decision 0045, item 2): a file on the device,
 * the last 2,000 lines, numbered. What happened while Cratarr could not be reached is kept for when it can: logcat
 * holds about a quarter of an hour on a phone, and on the owner's this app's lines did not reach it at all. On every
 * connect the lines Cratarr has not had yet go to it (`/api/players/{name}/log`), their times put on Cratarr's clock.
 */
object LinkLog {
    private const val TAG = "CratarrLink"
    private const val FILE = "link-log.jsonl"
    /** The log's name: a new one each time the file starts over, so Cratarr counts its lines anew. */
    private const val NAME = "link-log.name"
    private const val KEEP = 2_000
    /** Lines written past [KEEP] before the oldest are cut. */
    private const val SLACK = 200

    private val lock = Any()
    private var seq = -1L
    private var lines = 0

    /** One line: `level` info, warn or error; `area` one of Cratarr's (playon, syncplay…); `what` the same words each
     *  time, what varies in `fields`. Never a secret in it. */
    fun add(context: Context, level: String, area: String, what: String, vararg fields: Pair<String, Any?>) {
        val f = JSONObject()
        for ((k, v) in fields) if (v != null) f.put(k, v)
        when (level) {
            "error" -> Log.e(TAG, "$area: $what $f")
            "warn" -> Log.w(TAG, "$area: $what $f")
            else -> Log.i(TAG, "$area: $what $f")
        }
        val dir = context.applicationContext.filesDir
        synchronized(lock) {
            try {
                val file = File(dir, FILE)
                // A file that starts over gets a new name, so Cratarr counts its lines anew.
                if (!file.exists()) {
                    File(dir, NAME).writeText(UUID.randomUUID().toString())
                    seq = 0
                    lines = 0
                }
                if (seq < 0) load(file)
                seq++
                val line = JSONObject()
                    .put("seq", seq)
                    .put("at", System.currentTimeMillis())
                    .put("level", level)
                    .put("area", area)
                    .put("what", what)
                    .put("fields", f)
                file.appendText(line.toString() + "\n")
                lines++
                if (lines > KEEP + SLACK) trim(file)
            } catch (e: Exception) {
                Log.w(TAG, "cannot write the link log: ${e.message}")
            }
        }
    }

    /** Where the count stands: the last line's number, and how many lines the file holds. */
    private fun load(file: File) {
        val all = if (file.exists()) file.readLines().filter { it.isNotBlank() } else emptyList()
        seq = all.lastOrNull()?.let { runCatching { JSONObject(it).getLong("seq") }.getOrNull() } ?: 0L
        lines = all.size
    }

    /** The oldest lines cut, the last [KEEP] kept (written aside, then put in place). */
    private fun trim(file: File) {
        val kept = file.readLines().filter { it.isNotBlank() }.takeLast(KEEP)
        val aside = File(file.parentFile, "$FILE.new")
        aside.writeText(kept.joinToString("\n", postfix = "\n"))
        aside.renameTo(file)
        lines = kept.size
    }

    private fun name(dir: File): String {
        val f = File(dir, NAME)
        if (!f.exists()) f.writeText(UUID.randomUUID().toString())
        return f.readText().trim()
    }

    /**
     * The lines Cratarr has not had yet, to it: it says the last one it holds, and the ones after go, their times moved
     * onto Cratarr's clock by the offset known now (sync play's). `base`: the player's address on Cratarr. Blocking.
     */
    fun send(context: Context, base: String) {
        val dir = context.applicationContext.filesDir
        val (log, all) = synchronized(lock) {
            if (!File(dir, FILE).exists()) return
            name(dir) to File(dir, FILE).readLines().filter { it.isNotBlank() }
        }
        try {
            val asked = Net.open(context, "$base/log?log=${Uri.encode(log)}")
            asked.connectTimeout = 10_000
            asked.readTimeout = 10_000
            if (Net.code(context, asked) != 200) return
            val last = asked.inputStream.bufferedReader().use { JSONObject(it.readText()).optLong("last") }
            asked.disconnect()
            val offset = Clock.offsetMs.toLong()
            val out = JSONArray()
            for (l in all) {
                val j = runCatching { JSONObject(l) }.getOrNull() ?: continue
                if (j.optLong("seq") <= last) continue
                j.put("at", j.optLong("at") + offset)
                out.put(j)
            }
            if (out.length() == 0) return
            val c = Net.open(context, "$base/log")
            c.requestMethod = "POST"
            c.connectTimeout = 10_000
            c.readTimeout = 30_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(JSONObject().put("log", log).put("lines", out).toString().toByteArray(Charsets.UTF_8)) }
            Net.code(context, c)
            c.disconnect()
        } catch (e: Exception) {
            Log.w(TAG, "cannot send the link log: ${e.message}")
        }
    }

    /** A crash is written down before the app goes (then Android's own handling, as before). */
    fun catchCrashes(context: Context) {
        val app = context.applicationContext
        val before = Thread.getDefaultUncaughtExceptionHandler()
        if (before is Crashes) return
        Thread.setDefaultUncaughtExceptionHandler(Crashes(app, before))
    }

    private class Crashes(private val app: Context, private val before: Thread.UncaughtExceptionHandler?) : Thread.UncaughtExceptionHandler {
        override fun uncaughtException(t: Thread, e: Throwable) {
            try {
                val at = e.stackTrace.take(5).joinToString(" < ") { "${it.className.substringAfterLast('.')}.${it.methodName}:${it.lineNumber}" }
                add(app, "error", "playon", "crash", "thread" to t.name, "error" to e.toString(), "at" to at)
            } catch (_: Throwable) {
                // Nothing more to be done.
            }
            before?.uncaughtException(t, e)
        }
    }
}
