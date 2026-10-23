/*
 * Cratarr's own (decision 0033, step 4): the plays of this device's player, counted here and told to Cratarr, which
 * counts each once. A play is a track played 90% through, as the page counts (decision 0022), once each time it plays;
 * it keeps the time it was played. Told at once when Cratarr can be reached, else kept (30 days at most) and told when
 * the connection is back: plays made offline, or behind a locked screen, count too.
 * Since decision 0064 every track heard is told, not only those that count: a track left before it counts is told when
 * it is left (skipped for another, stopped when nothing follows), with how much of it was heard.
 */
package app.cratarr

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import java.util.concurrent.Executors

object Plays {
    private const val TAG = "CratarrPlays"
    private const val PENDING = "plays_pending"
    /** A track counts as played this far through. */
    const val PLAYED_AT = 0.9
    /** Plays not told after this long are given up. */
    private const val KEEP_MS = 30L * 24 * 3600 * 1000

    private val sender = Executors.newSingleThreadExecutor()

    /**
     * A track heard (as the page describes it), started at `atMs`: how it ended (`finished`, `skipped`, `stopped`),
     * whether it counts as a play, how much of it was heard and how long it is (ms; 0 when not known).
     */
    @Synchronized
    fun heard(context: Context, track: JSONObject, atMs: Long, ended: String, counts: Boolean, heardMs: Long, lengthMs: Long) {
        val source = track.optString("source")
        val id = track.optString("id")
        if (source.isEmpty() || id.isEmpty()) return
        val prefs = Prefs(context)
        val play = JSONObject()
            .put("id", UUID.randomUUID().toString())
            .put("source", source)
            .put("track", id)
            .put("at", atMs)
            .put("ended", ended)
            .put("counts", counts)
            .put("heard_secs", heardMs / 1000.0)
            .put("player", prefs.playerName)
        if (lengthMs > 0) play.put("length_secs", lengthMs / 1000.0)
        prefs.queueId.takeIf { it.isNotEmpty() }?.let { play.put("context", "queue:$it") }
        save(context, load(context).apply { add(play) })
        tell(context)
    }

    /** Tells Cratarr the plays not told yet; those it answers for are forgotten. */
    fun tell(context: Context) {
        val app = context.applicationContext
        sender.execute {
            val server = Prefs(app).serverUrl?.trimEnd('/') ?: return@execute
            val pending = synchronized(this) { load(app) }
            if (pending.isEmpty()) return@execute
            val done = try {
                post(app, "$server/api/plays", JSONObject().put("events", JSONArray(pending)).toString())
            } catch (e: Exception) {
                Log.i(TAG, "${pending.size} plays not told yet: ${e.message}")
                return@execute
            }
            synchronized(this) {
                val now = System.currentTimeMillis()
                save(app, load(app).filter { it.optString("id") !in done && now - it.optLong("at") < KEEP_MS }.toMutableList())
            }
        }
    }

    /** POSTs the plays; the ids Cratarr counted (now or before). */
    private fun post(app: Context, url: String, body: String): Set<String> {
        val c = Net.open(app, url)
        try {
            c.requestMethod = "POST"
            c.connectTimeout = 10_000
            c.readTimeout = 30_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            val code = Net.code(app, c)
            if (code !in 200..299) throw java.io.IOException("HTTP $code")
            val done = JSONObject(c.inputStream.bufferedReader().use { it.readText() }).optJSONArray("done") ?: JSONArray()
            return (0 until done.length()).map(done::optString).toSet()
        } finally {
            c.disconnect()
        }
    }

    private fun load(context: Context): MutableList<JSONObject> {
        val raw = Prefs.shared(context).getString(PENDING, null) ?: return mutableListOf()
        return try {
            val a = JSONArray(raw)
            (0 until a.length()).mapNotNull(a::optJSONObject).toMutableList()
        } catch (e: Exception) {
            mutableListOf()
        }
    }

    private fun save(context: Context, plays: List<JSONObject>) {
        Prefs.shared(context).edit().putString(PENDING, JSONArray(plays).toString()).apply()
    }
}
