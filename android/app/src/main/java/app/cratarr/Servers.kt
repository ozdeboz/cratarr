/*
 * Modes per server (decision 0098): the Jellyfin and Subsonic servers this device is signed in to, each a mode of its
 * own beside Cratarr's and Local. Their sign-ins are kept here, encrypted with a key Android's keystore holds (it never
 * leaves it), and never sent to Cratarr's server. The page's code for those servers (web/src/lib/servers) asks through
 * the app (`server-fetch`): its requests go out from here, so a page over https reaches a server on plain http at home,
 * no server has to allow the page's origin, and they carry nothing of Cratarr's own (no session, no cookie).
 * Each Cratarr account has its own (decision 0101, [Who]), hidden while another is signed in; those added with nobody
 * signed in are the device's no-account side's, so the app plays a server's music with no Cratarr account at all.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.media3.common.MediaItem
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.security.SecureRandom

object Servers {
    private const val TAG = "Servers"
    /** Where they are kept, sealed (one JSON object: each [Who] key's list). */
    private val sealed = Sealed("cratarr-servers", "servers")
    /** A mode's name for a server: `server:<its id>`. */
    const val MODE_PREFIX = "server:"
    /** The most of an answer read for the page. */
    private const val MOST = 32 * 1024 * 1024

    /** Every server the one signed in now keeps ([Who]), with its sign-in (`{id, kind, name, url, user, userId?,
     *  secret, apiKey?, scrobbles?}`), in the order they were added. None where they cannot be read (the keystore's
     *  key gone with a reset of the device's lock): they are signed in to again. */
    fun all(context: Context): JSONArray = everyone(context).optJSONArray(Who.current(context))?.let { JSONArray(it.toString()) } ?: JSONArray()

    private fun everyone(context: Context): JSONObject {
        cached?.let { return it }
        val text = sealed.read(context) ?: return JSONObject()
        return try {
            JSONObject(text).also { cached = it }
        } catch (e: Exception) {
            Log.w(TAG, "the kept servers cannot be read: ${e.javaClass.simpleName}")
            JSONObject()
        }
    }

    /** Everyone's lists as last read or written, so the player's every track does not decrypt them again. */
    @Volatile
    private var cached: JSONObject? = null

    @Synchronized
    private fun write(context: Context, list: JSONArray) {
        val all = JSONObject(everyone(context).toString()).put(Who.current(context), list)
        cached = all
        sealed.write(context, all.toString())
    }

    fun find(context: Context, id: String): JSONObject? {
        val list = all(context)
        return (0 until list.length()).map(list::getJSONObject).firstOrNull { it.optString("id") == id }
    }

    fun has(context: Context, id: String): Boolean = find(context, id) != null

    /** A server signed in to, or one changed (its sign-in again, its scrobbling switch): in place where it is kept. */
    fun keep(context: Context, server: JSONObject) {
        val id = server.optString("id")
        if (id.isEmpty()) return
        val list = all(context)
        val out = JSONArray()
        var placed = false
        for (i in 0 until list.length()) {
            val s = list.getJSONObject(i)
            if (s.optString("id") == id) {
                out.put(server)
                placed = true
            } else {
                out.put(s)
            }
        }
        if (!placed) out.put(server)
        write(context, out)
    }

    fun forget(context: Context, id: String) {
        val list = all(context)
        val out = JSONArray()
        for (i in 0 until list.length()) if (list.getJSONObject(i).optString("id") != id) out.put(list.getJSONObject(i))
        write(context, out)
    }

    /** A request of the page's for one of them (`{id, url, method?, headers?, body?}`), answered as
     *  `{type: "server-fetched", id, status, headers, body}`, or with `error` where the server was not reached. Only
     *  http and https; on the IO dispatcher. */
    fun fetch(message: JSONObject): JSONObject {
        val id = message.optString("id")
        val out = JSONObject().put("type", "server-fetched").put("id", id)
        val url = message.optString("url")
        if (!url.startsWith("https://") && !url.startsWith("http://")) return out.put("error", "not a web address")
        return try {
            val c = URL(url).openConnection() as HttpURLConnection
            c.connectTimeout = 15_000
            c.readTimeout = 30_000
            c.instanceFollowRedirects = true
            c.requestMethod = message.optString("method", "GET").uppercase()
            message.optJSONObject("headers")?.let { h -> h.keys().forEach { k -> c.setRequestProperty(k, h.optString(k)) } }
            if (message.has("body") && !message.isNull("body")) {
                c.doOutput = true
                c.outputStream.use { it.write(message.optString("body").toByteArray(Charsets.UTF_8)) }
            }
            val status = c.responseCode
            val stream = if (status >= 400) c.errorStream else c.inputStream
            val bytes = stream?.use { s ->
                val buf = java.io.ByteArrayOutputStream()
                val chunk = ByteArray(64 * 1024)
                while (true) {
                    val n = s.read(chunk)
                    if (n < 0) break
                    buf.write(chunk, 0, n)
                    if (buf.size() > MOST) throw IOException("the answer is too large")
                }
                buf.toByteArray()
            } ?: ByteArray(0)
            val headers = JSONObject()
            c.headerFields.forEach { (k, v) -> if (k != null && v.isNotEmpty()) headers.put(k.lowercase(), v.joinToString(", ")) }
            c.disconnect()
            out.put("status", status).put("headers", headers).put("body", String(bytes, Charsets.UTF_8))
        } catch (e: Exception) {
            out.put("error", e.message ?: e.javaClass.simpleName)
        }
    }

    /** The server a source names (`server:<id>`), as kept. */
    private fun ofSource(context: Context, source: String): JSONObject? =
        if (source.startsWith(MODE_PREFIX)) find(context, source.removePrefix(MODE_PREFIX)) else null

    /** Subsonic's sign-in for one request: the API key, or the user with a salted token of the password. */
    private fun subsonicAuth(s: JSONObject): String {
        if (s.optBoolean("apiKey")) return "apiKey=${Uri.encode(s.optString("secret"))}"
        val salt = ByteArray(8).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it) }
        val token = MessageDigest.getInstance("MD5").digest((s.optString("secret") + salt).toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }
        return "u=${Uri.encode(s.optString("user"))}&t=$token&s=$salt"
    }

    private fun base(s: JSONObject) = s.optString("url").trimEnd('/')

    /** Where the player streams a track from: the file as it is, with the sign-in in it (never shown, never logged). */
    fun streamUrl(s: JSONObject, track: String): String = when (s.optString("kind")) {
        "jellyfin" -> "${base(s)}/Audio/${Uri.encode(track)}/stream?static=true&api_key=${Uri.encode(s.optString("secret"))}&DeviceId=cratarr-${Uri.encode(s.optString("id"))}"
        else -> "${base(s)}/rest/stream.view?id=${Uri.encode(track)}&v=1.16.1&c=Cratarr&${subsonicAuth(s)}"
    }

    /** A cover, square at `size`. */
    fun coverUrl(s: JSONObject, cover: String, size: Int): String = when (s.optString("kind")) {
        "jellyfin" -> "${base(s)}/Items/${Uri.encode(cover)}/Images/Primary?fillWidth=$size&fillHeight=$size&quality=90&api_key=${Uri.encode(s.optString("secret"))}"
        else -> "${base(s)}/rest/getCoverArt.view?id=${Uri.encode(cover)}&size=$size&v=1.16.1&c=Cratarr&${subsonicAuth(s)}"
    }

    /** A server's track's stand-in address (`cratarr-server://{server}/{track}`): what the player and the downloads
     *  keep, so no sign-in is ever written down; turned into its stream as it is read ([resolve]). */
    const val SCHEME = "cratarr-server"

    fun streamUri(server: String, track: String): Uri = Uri.parse("$SCHEME://${Uri.encode(server)}/${Uri.encode(track)}")

    /** A stand-in address's stream, with the kept sign-in; null for any other address (or a server no longer kept). */
    fun resolve(context: Context, uri: Uri): Uri? {
        if (uri.scheme != SCHEME) return null
        val s = find(context, Uri.decode(uri.host ?: return null)) ?: return null
        val track = Uri.decode(uri.path?.removePrefix("/") ?: return null)
        return Uri.parse(streamUrl(s, track))
    }

    /** A server's track with its stream (its stand-in, stored whole where it was downloaded: the store's key is its
     *  media id) and its album's cover; null for any other. */
    fun located(context: Context, item: MediaItem): MediaItem? {
        val slash = item.mediaId.indexOf('/')
        if (slash < 0) return null
        val source = Uri.decode(item.mediaId.substring(0, slash))
        val s = ofSource(context, source) ?: return null
        val id = Uri.decode(item.mediaId.substring(slash + 1))
        val album = trackOf(item)?.optString("album_id")?.takeIf { it.isNotEmpty() && it != "null" }
        val b = item.buildUpon().setUri(streamUri(s.optString("id"), id)).setCustomCacheKey(item.mediaId)
        album?.let { b.setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(Uri.parse(coverUrl(s, it, 600))).build()) }
        return b.build()
    }

    /** `/__server/cover/{server}/{cover}?size=`: the picture and its type, fetched here with the kept sign-in; null
     *  where the server is not signed in to or has none. Off the main thread. */
    fun cover(context: Context, server: String, cover: String, size: Int): Pair<String, ByteArray>? {
        val s = find(context, server) ?: return null
        return try {
            val c = URL(coverUrl(s, cover, size.coerceIn(32, 1200))).openConnection() as HttpURLConnection
            c.connectTimeout = 10_000
            c.readTimeout = 20_000
            if (c.responseCode !in 200..299) return null.also { c.disconnect() }
            val type = c.contentType?.substringBefore(';')?.takeIf { it.startsWith("image/") } ?: "image/jpeg"
            val bytes = c.inputStream.use { it.readBytes() }
            c.disconnect()
            type to bytes
        } catch (e: IOException) {
            null
        }
    }

    private val reports = java.util.concurrent.Executors.newSingleThreadExecutor()

    /** Jellyfin's own way of naming a client, with the kept token. */
    private fun jellyfinAuth(context: Context, s: JSONObject) =
        "MediaBrowser Client=\"Cratarr\", Device=\"${Prefs(context).playerName}\", DeviceId=\"cratarr-${s.optString("id")}\", Version=\"${BuildConfig.VERSION_NAME}\", Token=\"${s.optString("secret")}\""

    /** One request of a report, best effort: false where it was not taken. */
    private fun send(url: String, method: String, auth: String?, body: String?): Boolean = try {
        val c = URL(url).openConnection() as HttpURLConnection
        c.connectTimeout = 10_000
        c.readTimeout = 15_000
        c.requestMethod = method
        auth?.let { c.setRequestProperty("Authorization", it) }
        if (body != null) {
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        }
        val ok = c.responseCode in 200..299
        c.disconnect()
        ok
    } catch (e: IOException) {
        false
    }

    /** That a server's track started playing here (its "now playing"), best effort, off the main thread. */
    fun playing(context: Context, source: String, track: String) {
        val s = ofSource(context, source) ?: return
        reports.execute {
            when (s.optString("kind")) {
                "jellyfin" -> send("${base(s)}/Sessions/Playing", "POST", jellyfinAuth(context, s), JSONObject().put("ItemId", track).put("CanSeek", true).toString())
                else -> send("${base(s)}/rest/scrobble.view?id=${Uri.encode(track)}&submission=false&v=1.16.1&c=Cratarr&f=json&${subsonicAuth(s)}", "GET", null, null)
            }
        }
    }

    /** A play of a server's track (played 90% through), told to that server at the time it was played, as that server
     *  counts plays; off the main thread. A play it did not take is not kept for later (its own history, best effort). */
    fun played(context: Context, source: String, track: String, atMs: Long) {
        val s = ofSource(context, source) ?: return
        reports.execute {
            val ok = when (s.optString("kind")) {
                "jellyfin" -> {
                    val at = Uri.encode(java.time.Instant.ofEpochMilli(atMs).toString())
                    val user = Uri.encode(s.optString("userId"))
                    // Jellyfin 10.9 on, then the one before it.
                    send("${base(s)}/UserPlayedItems/${Uri.encode(track)}?userId=$user&datePlayed=$at", "POST", jellyfinAuth(context, s), null) ||
                        send("${base(s)}/Users/$user/PlayedItems/${Uri.encode(track)}?datePlayed=$at", "POST", jellyfinAuth(context, s), null)
                }
                else -> send("${base(s)}/rest/scrobble.view?id=${Uri.encode(track)}&submission=true&time=$atMs&v=1.16.1&c=Cratarr&f=json&${subsonicAuth(s)}", "GET", null, null)
            }
            if (!ok) Log.w(TAG, "a play was not taken by ${s.optString("kind")}")
        }
    }

    /** One GET of a server's answer as JSON, with its sign-in; null where it was not answered. */
    private fun json(url: String, auth: String?): JSONObject? = try {
        val c = URL(url).openConnection() as HttpURLConnection
        c.connectTimeout = 10_000
        c.readTimeout = 20_000
        auth?.let { c.setRequestProperty("Authorization", it) }
        val text = if (c.responseCode in 200..299) c.inputStream.use { it.readBytes().toString(Charsets.UTF_8) } else null
        c.disconnect()
        text?.let(::JSONObject)
    } catch (e: Exception) {
        null
    }

    /**
     * That server's own mix from a track (decision 0098; the owner, 2026-11-03: Radio and Autoplay are "that server's own
     * mix"): Jellyfin's Instant Mix, Subsonic's similar songs; at most `count`, none already in `exclude`, as the page
     * describes tracks, in `lane`. Empty where the server has none. Off the main thread.
     */
    fun mix(context: Context, source: String, track: String, count: Int, exclude: Set<String>, lane: String): JSONArray {
        val s = ofSource(context, source) ?: return JSONArray()
        val out = JSONArray()
        fun put(id: String, name: String, artist: String?, album: String?, albumId: String?, secs: Double?) {
            if (id.isEmpty() || id in exclude || out.length() >= count) return
            out.put(
                JSONObject().put("source", source).put("id", id).put("name", name)
                    .put("artist", artist ?: JSONObject.NULL).put("album", album ?: JSONObject.NULL)
                    .put("album_id", albumId ?: JSONObject.NULL).put("duration_secs", secs ?: JSONObject.NULL).put("lane", lane),
            )
        }
        fun JSONObject.text(k: String) = optString(k).takeIf { it.isNotEmpty() && it != "null" }
        val ask = count + exclude.size.coerceAtMost(50)
        when (s.optString("kind")) {
            "jellyfin" -> {
                val items = json("${base(s)}/Items/${Uri.encode(track)}/InstantMix?userId=${Uri.encode(s.optString("userId"))}&limit=$ask", jellyfinAuth(context, s))?.optJSONArray("Items") ?: return out
                for (i in 0 until items.length()) {
                    val it = items.optJSONObject(i) ?: continue
                    val artist = it.optJSONArray("Artists")?.let { a -> (0 until a.length()).joinToString(", ") { n -> a.optString(n) } }?.ifEmpty { null } ?: it.text("AlbumArtist")
                    put(it.optString("Id"), it.optString("Name"), artist, it.text("Album"), it.text("AlbumId"), it.optLong("RunTimeTicks").takeIf { t -> t > 0 }?.let { t -> t / 10_000_000.0 })
                }
            }
            else -> {
                val songs = json("${base(s)}/rest/getSimilarSongs2.view?id=${Uri.encode(track)}&count=$ask&v=1.16.1&c=Cratarr&f=json&${subsonicAuth(s)}", null)
                    ?.optJSONObject("subsonic-response")?.optJSONObject("similarSongs2")?.optJSONArray("song") ?: return out
                for (i in 0 until songs.length()) {
                    val it = songs.optJSONObject(i) ?: continue
                    put(it.optString("id"), it.optString("title"), it.text("artist"), it.text("album"), it.text("albumId"), it.optDouble("duration").takeIf { d -> !d.isNaN() })
                }
            }
        }
        return out
    }
}
