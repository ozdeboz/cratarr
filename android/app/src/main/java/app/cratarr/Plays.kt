/*
 * Cratarr's own (decision 0033, step 4): the plays of this device's player, counted here and told to Cratarr, which
 * counts each once. A play is a track played 90% through, as the page counts (decision 0022), once each time it plays;
 * it keeps the time it was played. Told at once when Cratarr can be reached, else kept (30 days at most) and told when
 * the connection is back: plays made offline, or behind a locked screen, count too.
 */
package app.cratarr

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
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

    /** One play of a track (as the page describes it), now. */
    @Synchronized
    fun played(context: Context, track: JSONObject) {
        val source = track.optString("source")
        val id = track.optString("id")
        if (source.isEmpty() || id.isEmpty()) return
        val play = JSONObject()
            .put("id", UUID.randomUUID().toString())
            .put("source", source)
            .put("track", id)
            .put("at", System.currentTimeMillis())
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
                post("$server/api/plays", JSONObject().put("events", JSONArray(pending)).toString())
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
    private fun post(url: String, body: String): Set<String> {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = "POST"
            c.connectTimeout = 10_000
            c.readTimeout = 30_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            if (c.responseCode !in 200..299) throw java.io.IOException("HTTP ${c.responseCode}")
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
