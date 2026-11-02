/*
 * Cratarr's own (decision 0033, step 4): the plays of this device's player, counted here and told to Cratarr, which
 * counts each once. A play is a track played 90% through, as the page counts (decision 0022), once each time it plays;
 * it keeps the time it was played. Told at once when Cratarr can be reached, else kept (30 days at most) and told when
 * the connection is back: plays made offline, or behind a locked screen, count too.
 * Since decision 0064 every track heard is told, not only those that count: a track left before it counts is told when
 * it is left (skipped for another, stopped when nothing follows), with how much of it was heard.
 * Each kept play carries the account it was heard under, and is told only while that account is signed in (the owner,
 * 2026-11-02, a bug fixed in 0.13: plays kept while one account was signed in were told as the next one's); another
 * account's wait for it, 30 days at most.
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
    /** The account signed in as last learnt ([learn]): the plays heard from now on carry it. */
    private const val ACCOUNT = "plays_account"
    private const val PLAYED_HERE = "plays_device"
    /** The device music's plays kept here, the latest tracks. */
    private const val PLAYED_HERE_MOST = 200
    /** A track counts as played this far through. */
    const val PLAYED_AT = 0.9
    /** Plays not told after this long are given up. */
    private const val KEEP_MS = 30L * 24 * 3600 * 1000

    private val sender = Executors.newSingleThreadExecutor()

    /** Told (on the player's thread) when the device music played here changed. */
    @Volatile var onPlayedHere: (() -> Unit)? = null

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
        Prefs.shared(context).getString(ACCOUNT, null)?.let { play.put("account", it) }
        save(context, load(context).apply { add(play) })
        if (source == DeviceMusic.SOURCE && counts) playedHere(context, id, atMs)
        tell(context)
    }

    /**
     * Played here (0.9.15, local mode's Home; the owner, 2026-10-28: "the best PWA offline support possible"): the
     * device music this device played, kept here, as Cratarr cannot know it offline, and the app plays with the page
     * closed (in the background, Android Auto). Each track once: when last played, and how often.
     */
    @Synchronized
    private fun playedHere(context: Context, id: String, atMs: Long) {
        val all = loadArray(context, PLAYED_HERE)
        val was = all.firstOrNull { it.optString("id") == id }
        val count = (was?.optInt("count") ?: 0) + 1
        val now = all.filter { it !== was }.toMutableList()
        now.add(JSONObject().put("id", id).put("at", atMs).put("count", count))
        val kept = now.sortedByDescending { it.optLong("at") }.take(PLAYED_HERE_MOST)
        Prefs.shared(context).edit().putString(PLAYED_HERE, JSONArray(kept).toString()).apply()
        onPlayedHere?.invoke()
    }

    /** The page's `device-plays` message: the device music played here, the latest first. */
    @Synchronized
    fun playedHere(context: Context): JSONObject =
        JSONObject().put("type", "device-plays").put("plays", JSONArray(loadArray(context, PLAYED_HERE)))

    /** The page signed in (another session): whose it is, learnt now, so the plays heard from now on carry it; then that
     *  account's own plays are told. Signed out (no session): the account is forgotten, so plays heard before the next
     *  one is learnt wait unstamped and take its key, not the one signed out's (632's review, 2026-11-02); asked,
     *  Cratarr would only refuse. */
    fun signedIn(context: Context) {
        val app = context.applicationContext
        val server = Prefs(app).serverUrl?.trimEnd('/') ?: return
        if (Sessions.tokenIn(Net.cookie(server)) == null) {
            Prefs.shared(app).edit().remove(ACCOUNT).apply()
            return
        }
        sender.execute { learn(app, server) }
        tell(app)
    }

    /** Tells Cratarr the plays not told yet of the account signed in; those it answers for are forgotten. */
    fun tell(context: Context) {
        val app = context.applicationContext
        sender.execute {
            val server = Prefs(app).serverUrl?.trimEnd('/') ?: return@execute
            if (synchronized(this) { load(app) }.isEmpty()) return@execute
            val account = learn(app, server) ?: return@execute
            val pending = synchronized(this) {
                val now = System.currentTimeMillis()
                val kept = stamped(load(app), account).filter { ours(it, account) || now - it.optLong("at") < KEEP_MS }
                save(app, kept)
                kept.filter { ours(it, account) }
            }
            if (pending.isEmpty()) return@execute
            val done = try {
                post(app, "$server/api/plays", JSONObject().put("events", JSONArray(pending.map(::told))).toString())
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

    /** The account signed in, as Cratarr's `/api/me` says (blocking), kept as the last learnt; null when it did not say
     *  (not reached, not signed in): nothing is told then. */
    private fun learn(app: Context, server: String): String? {
        val account = try {
            val c = Net.open(app, "$server/api/me")
            try {
                c.connectTimeout = 10_000
                c.readTimeout = 10_000
                val code = Net.code(app, c)
                val body = (if (code in 200..299) c.inputStream else c.errorStream)?.bufferedReader()?.use { it.readText() }
                accountOf(code, body)
            } finally {
                c.disconnect()
            }
        } catch (e: Exception) {
            null
        }
        if (account == null) Log.i(TAG, "whose the plays are not known yet") else Prefs.shared(app).edit().putString(ACCOUNT, account).apply()
        return account
    }

    /** The account a `/api/me` answer names: its name, or "" with signing in off (one user); null when it names none. */
    fun accountOf(code: Int, body: String?): String? = when {
        code in 200..299 -> body?.let { runCatching { JSONObject(it).getString("name") }.getOrNull() }
        code == 404 && body?.contains("Signing in is off") == true -> ""
        else -> null
    }

    /** The account known: the plays carrying none take it (heard before one was learnt here, or kept before 0.13: the
     *  account signed in then, as was taken until now). */
    fun stamped(plays: List<JSONObject>, account: String): MutableList<JSONObject> =
        plays.map { if (it.has("account")) it else JSONObject(it.toString()).put("account", account) }.toMutableList()

    /** Whether a play is [account]'s: only those are told. */
    fun ours(play: JSONObject, account: String): Boolean = play.has("account") && play.optString("account") == account

    /** A play as Cratarr is told it: without the account it waited for. */
    private fun told(play: JSONObject): JSONObject = JSONObject(play.toString()).apply { remove("account") }

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

    private fun load(context: Context): MutableList<JSONObject> = loadArray(context, PENDING)

    private fun loadArray(context: Context, key: String): MutableList<JSONObject> {
        val raw = Prefs.shared(context).getString(key, null) ?: return mutableListOf()
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
