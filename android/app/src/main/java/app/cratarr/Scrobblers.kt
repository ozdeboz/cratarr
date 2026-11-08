/*
 * Scrobbling from the device (decision 0091; the owner, 2026-10-31: "scrobbling should indeed move to the device! This
 * also makes more sense privacy wise"): the app's player, which plays with the page closed, tells ListenBrainz and
 * Last.fm itself what it played, with this device's own sign-ins, sealed here and never sent to Cratarr's server. As the
 * page does in a browser (web/src/lib/scrobbleDevice, lib/scrobbles): each play once, by its id; Last.fm within 14 days;
 * a queue kept while a service is down. The account's way (each device, or through the server) and its tracking are as
 * the page last told them. Through the server, the plays of other servers' tracks are handed to Cratarr's server
 * (`/api/scrobbles`), which counts its own tracks' plays itself. A server whose "This server scrobbles for me" is on
 * (decision 0091, amended) keeps its tracks' scrobbles: none from here.
 * Each Cratarr account has its own here (decision 0101, [Who]): its sign-ins, its way and tracking, its queues, each
 * play going out for the account that played it. With nobody signed in, the device's no-account side's own services
 * only, from here, never an account's.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.Executors

object Scrobblers {
    private const val TAG = "Scrobblers"
    private val sealed = Sealed("cratarr-scrobblers", "scrobblers")
    private const val WAY = "scrobble_way"
    private const val TRACKING = "scrobble_tracking"
    private const val QUEUE = "scrobble_queue"
    private const val HANDOVER = "scrobble_handover"
    private const val LASTFM_MOST_S = 14L * 86_400
    private const val LB = "https://api.listenbrainz.org/1"
    private const val LFM = "https://ws.audioscrobbler.com/2.0/"
    /** Last.fm's sign-in comes back here (the app's own link), its token with it. */
    const val LASTFM_BACK = "cratarr://connected/lastfm"
    private val sender = Executors.newSingleThreadExecutor()

    /** Told (on the sender's thread) when what the page shows of them changed. */
    @Volatile var onChanged: (() -> Unit)? = null

    /** Everyone's sign-ins, each [Who] key's apart. */
    private fun everyone(context: Context): JSONObject = sealed.read(context)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()
    /** The sign-ins of the one signed in now, or of [who]: what goes out to a service is the account's it was read for,
     *  whoever has signed in by the time the service answers (a sign-out waits for it a few seconds at most). */
    private fun kept(context: Context, who: String = Who.current(context)): JSONObject = everyone(context).optJSONObject(who) ?: JSONObject()
    @Synchronized
    private fun seal(context: Context, k: JSONObject, who: String = Who.current(context)) {
        sealed.write(context, everyone(context).put(who, k).toString())
    }
    private fun keep(context: Context, k: JSONObject, who: String = Who.current(context)) {
        seal(context, k, who)
        onChanged?.invoke()
    }

    /** The account's way and its tracking, as the page last read them. */
    fun setWay(context: Context, way: String, tracking: Boolean) {
        Prefs.shared(context).edit().putString(Who.keyed(context, WAY), if (way == "device") "device" else "server").putBoolean(Who.keyed(context, TRACKING), tracking).apply()
        if (!tracking) Prefs.shared(context).edit().remove(Who.keyed(context, QUEUE)).remove(Who.keyed(context, HANDOVER)).apply()
        else send(context)
    }
    /** Until the page has told it, an account's way is what a new account's is (decision 0091): the private choice. */
    private fun way(context: Context) = if (Who.current(context) == Who.NONE) "device" else Prefs.shared(context).getString(Who.keyed(context, WAY), "device")
    private fun tracking(context: Context) = Prefs.shared(context).getBoolean(Who.keyed(context, TRACKING), true)

    /** What the page shows: each service signed in to here (whose, why it stopped), and how many plays wait. */
    fun here(context: Context): JSONObject {
        val k = kept(context)
        fun one(s: String) = k.optJSONObject(s)?.let { JSONObject().put("name", it.optString("name")).put("broken", it.opt("broken") ?: JSONObject.NULL) } ?: JSONObject.NULL
        val queued = queue(context).optJSONArray("plays")?.length() ?: 0
        return JSONObject().put("type", "scrobblers").put("here", JSONObject().put("listenbrainz", one("listenbrainz")).put("lastfm", one("lastfm"))).put("queued", queued)
    }

    fun keepListenBrainz(context: Context, token: String, name: String) {
        val k = kept(context)
        k.put("listenbrainz", JSONObject().put("token", token).put("name", name))
        keep(context, k)
        send(context)
    }

    fun forget(context: Context, service: String) {
        val k = kept(context)
        k.remove(service)
        keep(context, k)
    }

    /** Last.fm: the key and the secret kept for the session to come; its sign-in page to open, coming back to the app. */
    fun lastFmStart(context: Context, key: String, secret: String): String {
        val k = kept(context)
        k.put("lastfm_pending", JSONObject().put("key", key).put("secret", secret))
        seal(context, k)
        return "https://www.last.fm/api/auth/?api_key=${Uri.encode(key)}&cb=${Uri.encode(LASTFM_BACK)}"
    }

    /** Back from Last.fm's sign-in with its token: traded for the session (off the main thread); what went wrong, if
     *  anything. */
    fun lastFmBack(context: Context, token: String): String? {
        val who = Who.current(context)
        val k = kept(context, who)
        val p = k.optJSONObject("lastfm_pending") ?: return "Give Last.fm's key and secret first"
        return try {
            val r = lfm(p.optString("key"), p.optString("secret"), "auth.getSession", mapOf("token" to token))
            val session = r.optJSONObject("session") ?: return "Last.fm gave no session"
            k.remove("lastfm_pending")
            k.put("lastfm", JSONObject().put("key", p.optString("key")).put("secret", p.optString("secret")).put("session", session.optString("key")).put("name", session.optString("name")))
            keep(context, k, who)
            send(context)
            null
        } catch (e: Refused) {
            e.message
        }
    }

    /** A play this device's player counted (decision 0022), as the page describes its track, at `atMs`. */
    fun played(context: Context, track: JSONObject, id: String, atMs: Long, lengthMs: Long) {
        if (!tracking(context)) return
        val play = playOf(track, atMs / 1000, lengthMs)?.put("id", id) ?: return
        val source = track.optString("source")
        if (scrobbledByItsServer(context, source)) return
        when {
            way(context) == "device" -> add(context, QUEUE, play)
            // Through the server: another server's play handed to it; Cratarr's own it counts itself.
            source.startsWith(Servers.MODE_PREFIX) -> add(context, HANDOVER, play)
            else -> return
        }
        send(context)
    }

    /** What starts playing, as now playing: best effort, never queued. */
    fun playing(context: Context, track: JSONObject) {
        if (!tracking(context)) return
        val play = playOf(track, System.currentTimeMillis() / 1000, 0) ?: return
        val source = track.optString("source")
        if (scrobbledByItsServer(context, source)) return
        val app = context.applicationContext
        sender.execute {
            try {
                if (way(app) == "device") {
                    val k = kept(app)
                    k.optJSONObject("listenbrainz")?.takeIf { it.isNull("broken") || !it.has("broken") }?.let { lbSubmit(it.optString("token"), JSONObject().put("listen_type", "playing_now").put("payload", JSONArray().put(lbListen(play, false)))) }
                    k.optJSONObject("lastfm")?.takeIf { !it.has("broken") || it.isNull("broken") }?.let { l ->
                        lfm(l.optString("key"), l.optString("secret"), "track.updateNowPlaying", buildMap {
                            put("sk", l.optString("session")); put("artist", play.optString("artist")); put("track", play.optString("title"))
                            play.optString("album").takeIf { it.isNotEmpty() }?.let { put("album", it) }
                        })
                    }
                } else if (source.startsWith(Servers.MODE_PREFIX)) {
                    toServer(app, "/api/scrobbles/now", JSONObject().put("artist", play.optString("artist")).put("title", play.optString("title")).put("album", play.optString("album")))
                }
            } catch (e: Exception) {
                Log.i(TAG, "now playing not taken: ${e.message}")
            }
        }
    }

    private fun scrobbledByItsServer(context: Context, source: String): Boolean =
        source.startsWith(Servers.MODE_PREFIX) && Servers.find(context, source.removePrefix(Servers.MODE_PREFIX))?.optBoolean("scrobbles") == true

    private fun playOf(t: JSONObject, at: Long, lengthMs: Long): JSONObject? {
        val artist = t.optString("artist").takeIf { it.isNotEmpty() && it != "null" } ?: return null
        val title = t.optString("name").takeIf { it.isNotEmpty() } ?: return null
        return JSONObject().put("id", UUID.randomUUID().toString()).put("at", at).put("artist", artist).put("title", title)
            .put("album", t.optString("album").takeIf { it.isNotEmpty() && it != "null" } ?: "")
            .put("duration_secs", if (lengthMs > 0) lengthMs / 1000 else t.optLong("duration_secs"))
    }

    private fun queue(context: Context, who: String = Who.current(context)): JSONObject = Prefs.shared(context).getString(QUEUE + who, null)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()

    @Synchronized
    private fun add(context: Context, store: String, play: JSONObject) {
        val q = Prefs.shared(context).getString(Who.keyed(context, store), null)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()
        val plays = q.optJSONArray("plays") ?: JSONArray()
        plays.put(play)
        // At most the latest 5000 waiting.
        while (plays.length() > 5000) plays.remove(0)
        Prefs.shared(context).edit().putString(Who.keyed(context, store), q.put("plays", plays).toString()).apply()
    }

    /** Sent now, waiting for it a few seconds at most (blocking; the account signing out, decision 0101). */
    fun flush(context: Context) {
        send(context)
        runCatching { sender.submit {}.get(8, java.util.concurrent.TimeUnit.SECONDS) }
    }

    /** What waits, sent: to each service signed in to here, in batches, once each; through the server, handed over. */
    fun send(context: Context) {
        val app = context.applicationContext
        sender.execute {
            if (!tracking(app)) return@execute
            sendHere(app)
            handOver(app)
            onChanged?.invoke()
        }
    }

    @Synchronized
    private fun sendHere(context: Context) {
        val who = Who.current(context)
        val q = queue(context, who)
        val plays = q.optJSONArray("plays") ?: return
        val sent = q.optJSONObject("sent") ?: JSONObject()
        val k = kept(context, who)
        val now = System.currentTimeMillis() / 1000
        val services = listOf("listenbrainz", "lastfm").filter { s -> k.optJSONObject(s)?.let { !it.has("broken") || it.isNull("broken") } == true }
        for (s in services) {
            val had = (sent.optJSONArray(s) ?: JSONArray()).let { a -> (0 until a.length()).map { a.optString(it) }.toMutableSet() }
            val most = if (s == "lastfm") 50 else 1000
            while (true) {
                val batch = (0 until plays.length()).map(plays::getJSONObject)
                    .filter { it.optString("id") !in had && (s != "lastfm" || now - it.optLong("at") <= LASTFM_MOST_S) }
                    .take(most)
                if (batch.isEmpty()) break
                try {
                    if (s == "listenbrainz") {
                        val listens = JSONArray().apply { batch.forEach { put(lbListen(it, true)) } }
                        lbSubmit(k.getJSONObject(s).optString("token"), JSONObject().put("listen_type", if (batch.size == 1) "single" else "import").put("payload", listens))
                    } else {
                        val l = k.getJSONObject(s)
                        val p = mutableMapOf("sk" to l.optString("session"))
                        batch.forEachIndexed { i, b ->
                            p["artist[$i]"] = b.optString("artist"); p["track[$i]"] = b.optString("title"); p["timestamp[$i]"] = b.optLong("at").toString()
                            b.optString("album").takeIf { it.isNotEmpty() }?.let { p["album[$i]"] = it }
                            b.optLong("duration_secs").takeIf { it > 0 }?.let { p["duration[$i]"] = it.toString() }
                        }
                        lfm(l.optString("key"), l.optString("secret"), "track.scrobble", p)
                    }
                    had.addAll(batch.map { it.optString("id") })
                } catch (e: Refused) {
                    if (e.signedOut) {
                        k.optJSONObject(s)?.put("broken", e.message)
                        keep(context, k, who)
                    }
                    break
                } catch (e: Exception) {
                    break
                }
            }
            sent.put(s, JSONArray(had.toList()))
        }
        // Left: what a service signed in to here has not had and still can.
        val left = JSONArray()
        for (i in 0 until plays.length()) {
            val p = plays.getJSONObject(i)
            val wanted = services.any { s -> (sent.optJSONArray(s)?.let { a -> (0 until a.length()).none { a.optString(it) == p.optString("id") } } ?: true) && (s != "lastfm" || now - p.optLong("at") <= LASTFM_MOST_S) }
            if (wanted || services.isEmpty()) left.put(p)
        }
        val ids = (0 until left.length()).map { left.getJSONObject(it).optString("id") }.toSet()
        val kept = JSONObject()
        for (s in services) kept.put(s, JSONArray((sent.optJSONArray(s)?.let { a -> (0 until a.length()).map { a.optString(it) } } ?: emptyList()).filter { it in ids }))
        Prefs.shared(context).edit().putString(QUEUE + who, JSONObject().put("plays", left).put("sent", kept).toString()).apply()
    }

    @Synchronized
    private fun handOver(context: Context) {
        val who = Who.current(context)
        val q = Prefs.shared(context).getString(HANDOVER + who, null)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: return
        val plays = q.optJSONArray("plays") ?: return
        if (plays.length() == 0) return
        val batch = JSONArray()
        for (i in 0 until minOf(500, plays.length())) {
            val p = plays.getJSONObject(i)
            batch.put(JSONObject().put("id", p.optString("id")).put("at", p.optLong("at")).put("artist", p.optString("artist")).put("title", p.optString("title")).put("album", p.optString("album")).put("duration_secs", p.optLong("duration_secs")))
        }
        val ok = try {
            toServer(context, "/api/scrobbles", JSONObject().put("plays", batch))
        } catch (e: Exception) {
            false
        }
        if (!ok) return
        val rest = JSONArray()
        for (i in batch.length() until plays.length()) rest.put(plays.get(i))
        Prefs.shared(context).edit().putString(HANDOVER + who, JSONObject().put("plays", rest).toString()).apply()
    }

    private fun toServer(context: Context, path: String, body: JSONObject): Boolean {
        val base = Prefs(context).serverUrl?.trimEnd('/') ?: return false
        val c = Net.open(context, "$base$path")
        c.requestMethod = "POST"
        c.doOutput = true
        c.connectTimeout = 10_000
        c.readTimeout = 20_000
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
        val code = Net.code(context, c)
        c.disconnect()
        return code in 200..299
    }

    private class Refused(message: String, val signedOut: Boolean) : Exception(message)

    private fun lbListen(p: JSONObject, at: Boolean): JSONObject {
        val info = JSONObject().put("submission_client", "Cratarr")
        p.optLong("duration_secs").takeIf { it > 0 }?.let { info.put("duration_ms", it * 1000) }
        val meta = JSONObject().put("artist_name", p.optString("artist")).put("track_name", p.optString("title")).put("additional_info", info)
        p.optString("album").takeIf { it.isNotEmpty() }?.let { meta.put("release_name", it) }
        return JSONObject().apply { if (at) put("listened_at", p.optLong("at")) }.put("track_metadata", meta)
    }

    private fun lbSubmit(token: String, body: JSONObject) {
        val c = URL("$LB/submit-listens").openConnection() as HttpURLConnection
        c.requestMethod = "POST"
        c.doOutput = true
        c.connectTimeout = 10_000
        c.readTimeout = 20_000
        c.setRequestProperty("Authorization", "Token $token")
        c.setRequestProperty("Content-Type", "application/json")
        c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
        val code = c.responseCode
        val text = (if (code >= 400) c.errorStream else c.inputStream)?.use { it.readBytes().toString(Charsets.UTF_8) }.orEmpty()
        c.disconnect()
        if (code !in 200..299) throw Refused(runCatching { JSONObject(text).optString("error") }.getOrNull()?.ifEmpty { null } ?: "ListenBrainz refused it ($code)", code == 401)
    }

    /** Last.fm's signature: every parameter but `format`, sorted, each name then value, the secret last, MD5'd. */
    private fun signature(params: Map<String, String>, secret: String): String {
        val text = params.filterKeys { it != "format" && it != "callback" }.toSortedMap().entries.joinToString("") { it.key + it.value } + secret
        return MessageDigest.getInstance("MD5").digest(text.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }
    }

    private fun lfm(key: String, secret: String, method: String, params: Map<String, String>): JSONObject {
        val all = params + mapOf("method" to method, "api_key" to key)
        val body = (all + mapOf("api_sig" to signature(all, secret), "format" to "json")).entries.joinToString("&") { Uri.encode(it.key) + "=" + Uri.encode(it.value) }
        val c = URL(LFM).openConnection() as HttpURLConnection
        c.requestMethod = "POST"
        c.doOutput = true
        c.connectTimeout = 10_000
        c.readTimeout = 20_000
        c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
        c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        val code = c.responseCode
        val text = (if (code >= 400) c.errorStream else c.inputStream)?.use { it.readBytes().toString(Charsets.UTF_8) }.orEmpty()
        c.disconnect()
        val r = runCatching { JSONObject(text) }.getOrNull() ?: JSONObject()
        if (r.has("error")) throw Refused(r.optString("message").ifEmpty { "Last.fm refused it" }, r.optInt("error") in setOf(4, 9, 10, 14, 26))
        if (code !in 200..299) throw Refused("Last.fm refused it ($code)", false)
        return r
    }
}
