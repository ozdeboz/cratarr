/*
 * Modes per server (decision 0098): the Jellyfin and Subsonic servers this device is signed in to, each a mode of its
 * own beside Cratarr's and Local. Their sign-ins are kept here, encrypted with a key Android's keystore holds (it never
 * leaves it), and never sent to Cratarr's server. The page's code for those servers (web/src/lib/servers) asks through
 * the app (`server-fetch`): its requests go out from here, so a page over https reaches a server on plain http at home,
 * no server has to allow the page's origin, and they carry nothing of Cratarr's own (no session, no cookie).
 */
package app.cratarr

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

object Servers {
    private const val TAG = "Servers"
    /** The keystore's key for them. */
    private const val KEY = "cratarr-servers"
    /** Where they are kept, encrypted (one JSON list: base64 of the IV and the ciphertext). */
    private const val STORE = "servers"
    /** A mode's name for a server: `server:<its id>`. */
    const val MODE_PREFIX = "server:"
    /** The most of an answer read for the page. */
    private const val MOST = 32 * 1024 * 1024

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(KEY, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(KEY, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    /** Every server kept, with its sign-in (`{id, kind, name, url, user, userId?, secret, apiKey?, scrobbles?}`), in
     *  the order they were added. None where they cannot be read (the keystore's key gone with a reset of the device's
     *  lock): they are signed in to again. */
    fun all(context: Context): JSONArray {
        val kept = Prefs.shared(context).getString(STORE, null) ?: return JSONArray()
        return try {
            val raw = Base64.decode(kept, Base64.NO_WRAP)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12))
            JSONArray(String(cipher.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8))
        } catch (e: Exception) {
            Log.w(TAG, "the kept servers cannot be read: ${e.javaClass.simpleName}")
            JSONArray()
        }
    }

    private fun write(context: Context, list: JSONArray) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val out = cipher.iv + cipher.doFinal(list.toString().toByteArray(Charsets.UTF_8))
        Prefs.shared(context).edit().putString(STORE, Base64.encodeToString(out, Base64.NO_WRAP)).apply()
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
}
