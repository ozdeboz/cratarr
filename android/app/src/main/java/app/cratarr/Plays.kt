/*
 * Cratarr's own (decision 0033, step 4): the plays of this device's player, counted here and told to Cratarr, which
 * counts each once. A play is a track played 90% through, as the page counts (decision 0022), once each time it plays;
 * it keeps the time it was played. Told at once when Cratarr can be reached, else kept (30 days at most) and told when
 * the connection is back: plays made offline, or behind a locked screen, count too.
 * Since decision 0064 every track heard is told, not only those that count: a track left before it counts is told when
 * it is left (skipped for another, stopped when nothing follows), with how much of it was heard.
 * Each kept play carries the account it was heard under, and is told only while that account is signed in (the owner,
 * 2026-11-02, a bug fixed in 0.13: plays kept while one account was signed in were told as the next one's); another
 * account's wait for it, 30 days at most. One heard with nobody signed in (decision 0101) is counted on this device
 * only, never told as anyone's.
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
    private const val PLAYED_HERE = "plays_device"
    /** The device music's plays kept here, the latest tracks. */
    private const val PLAYED_HERE_MOST = 200
    /** A track counts as played this far through. */
    const val PLAYED_AT = 0.9
    /** Plays not told after this long are given up. */
    private const val KEEP_MS = 30L * 24 * 3600 * 1000

    private val sender = Executors.newSingleThreadExecutor()

    /** The plays of the account signed in not told yet: what this device adds to the counts it last heard. */
    @Synchronized
    fun waiting(context: Context): JSONArray {
        val who = Who.accountOf(Who.current(context))
        return JSONArray(load(context).filter { who != null && ours(it, who) }.map(::told))
    }

    /** Told (on the player's thread) when the device music played here changed. */
    @Volatile var onPlayedHere: (() -> Unit)? = null

    /**
     * A track heard (as the page describes it) under [who], started at `atMs`: how it ended (`finished`, `skipped`,
     * `stopped`), whether it counts as a play, how much of it was heard and how long it is (ms; 0 when not known). [who], not
     * the one signed in now: the track being left at a change is told just after it.
     */
    @Synchronized
    fun heard(context: Context, who: String, track: JSONObject, atMs: Long, ended: String, counts: Boolean, heardMs: Long, lengthMs: Long) {
        val source = track.optString("source")
        val id = track.optString("id")
        if (source.isEmpty() || id.isEmpty()) return
        // The play's own id, its report's and its scrobble's (decision 0091: one play sent once, whichever way).
        val playId = UUID.randomUUID().toString()
        if (counts) Scrobblers.played(context, track, playId, atMs, lengthMs)
        // A server's track (decision 0098): its play is that server's, never Cratarr's.
        if (source.startsWith(Servers.MODE_PREFIX)) {
            if (counts) Servers.played(context, source, id, atMs)
            return
        }
        val prefs = Prefs(context)
        val play = JSONObject()
            .put("id", playId)
            .put("source", source)
            .put("track", id)
            .put("at", atMs)
            .put("ended", ended)
            .put("counts", counts)
            .put("heard_secs", heardMs / 1000.0)
            .put("player", prefs.playerName)
        if (lengthMs > 0) play.put("length_secs", lengthMs / 1000.0)
        prefs.queueId.takeIf { it.isNotEmpty() && who == Who.current(context) }?.let { play.put("context", "queue:$it") }
        // Whose it is; with nobody, the device's own count only.
        val account = Who.accountOf(who)
        if (account != null) save(context, load(context).apply { add(play.put("account", account)) })
        if (source == DeviceMusic.SOURCE && counts) playedHere(context, id, atMs)
        if (account != null) tell(context)
    }

    /**
     * Played here (0.9.15, local mode's Home; the owner, 2026-10-28: "the best PWA offline support possible"): the
     * device music this device played, kept here, as Cratarr cannot know it offline, and the app plays with the page
     * closed (in the background, Android Auto). Each track once: when last played, and how often. Each account's its own.
     */
    @Synchronized
    private fun playedHere(context: Context, id: String, atMs: Long) {
        val all = loadArray(context, Who.keyed(context, PLAYED_HERE))
        val was = all.firstOrNull { it.optString("id") == id }
        val count = (was?.optInt("count") ?: 0) + 1
        val now = all.filter { it !== was }.toMutableList()
        now.add(JSONObject().put("id", id).put("at", atMs).put("count", count))
        val kept = now.sortedByDescending { it.optLong("at") }.take(PLAYED_HERE_MOST)
        Prefs.shared(context).edit().putString(Who.keyed(context, PLAYED_HERE), JSONArray(kept).toString()).apply()
        onPlayedHere?.invoke()
    }

    /** The page's `device-plays` message: the device music played here, the latest first. */
    @Synchronized
    fun playedHere(context: Context): JSONObject =
        JSONObject().put("type", "device-plays").put("plays", JSONArray(loadArray(context, Who.keyed(context, PLAYED_HERE))))

    /** The page signed in (another session): whose it is, learnt now ([Who]), so the plays heard from now on carry it;
     *  then that account's own plays are told. No session: nobody is signed in. */
    fun signedIn(context: Context) {
        val app = context.applicationContext
        val server = Prefs(app).serverUrl?.trimEnd('/') ?: return
        if (Sessions.tokenIn(Net.cookie(server)) == null) {
            // With sign-in off there is never one, and a move between the addresses is no sign-out.
            if (Who.current(app) != Who.keyOf("")) Who.set(app, null)
            return
        }
        sender.execute { learn(app, server) }
        tell(app)
    }

    /** Asks Cratarr whose this device is now (blocking), so [Who] is right before what goes by it opens. Not reached: as
     *  it was. */
    fun look(context: Context) {
        val app = context.applicationContext
        learn(app, Prefs(app).serverUrl?.trimEnd('/') ?: return)
    }

    /** Tells them now and waits for it, a few seconds at most (blocking; the account signing out, decision 0101: its
     *  plays go first, where Cratarr can be reached). */
    fun flush(context: Context) {
        tell(context)
        runCatching { sender.submit {}.get(8, java.util.concurrent.TimeUnit.SECONDS) }
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
                val kept = load(app).filter { ours(it, account) || now - it.optLong("at") < KEEP_MS }
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

    /** The account signed in, as Cratarr's `/api/me` says (blocking), this device's from now on ([Who]); null when it
     *  did not say (not reached; nobody signed in, which [Who] learns too): nothing is told then. An answer that no
     *  longer [stands] changes nothing here, and its account's plays still go: Cratarr can still hear them. */
    private fun learn(app: Context, server: String): String? {
        val asked = Who.current(app)
        var nobody = false
        val account = try {
            val c = Net.open(app, "$server/api/me")
            try {
                c.connectTimeout = 10_000
                c.readTimeout = 10_000
                val code = Net.code(app, c)
                val body = (if (code in 200..299) c.inputStream else c.errorStream)?.bufferedReader()?.use { it.readText() }
                nobody = signedOut(code, body, c.getHeaderField("WWW-Authenticate"))
                accountOf(code, body)
            } finally {
                c.disconnect()
            }
        } catch (e: Exception) {
            null
        }
        when {
            !stands(asked, Who.current(app), Sessions.signingOut) -> Log.i(TAG, "an answer from before a change")
            account != null -> Who.set(app, account)
            nobody -> Who.set(app, null)
            else -> Log.i(TAG, "whose the plays are not known yet")
        }
        return account
    }

    /** Whether an answer still says who is signed in: nothing moved since it was asked, with [asked] signed in then and
     *  [now] since, and no sign-out under way ([signingOut]), where the session it names stands until the page ends it.
     *  Taken up all the same, it signs the one leaving back in. */
    fun stands(asked: String, now: String, signingOut: Boolean): Boolean = asked == now && !signingOut

    /** The account a `/api/me` answer names: its name, or "" with signing in off (one user); null when it names none. */
    fun accountOf(code: Int, body: String?): String? = when {
        code in 200..299 -> body?.let { runCatching { JSONObject(it).getString("name") }.getOrNull() }
        code == 404 && body?.contains("Signing in is off") == true -> ""
        else -> null
    }

    /** A `/api/me` answer saying nobody is signed in (not one that only did not come). The proxy's bare 403, away from
     *  home, says nothing of the kind, nor any 401 but Cratarr's own ([challenge]: its `WWW-Authenticate`): taken for it, the
     *  account's downloads and queue would go just when they are all there is. */
    fun signedOut(code: Int, body: String?, challenge: String?): Boolean =
        (code == 401 && Sessions.asksSignIn(challenge)) || (code == 404 && body?.contains("No one is signed in") == true)

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
