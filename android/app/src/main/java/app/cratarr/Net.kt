/*
 * Cratarr's sign-in (decision 0039): away from home the reverse proxy lets through only a signed-in device's requests,
 * signed in on Cratarr's own sign-in page, which the app shows in its WebView like any other page. The session is the
 * WebView's cookie, and the app's own requests (streams, downloads, Play On) carry it too; the app keeps no password.
 * A refusal (401: not signed in, or the session ended; 403: not let in) holds the app's requests to the public address
 * back for a while, until the session changes (the page signed in again): asking again and again would get the phone
 * banned as a prober (it was, by CrowdSec, 2026-10-16).
 */
package app.cratarr

import android.content.Context
import android.os.SystemClock
import android.util.Log
import android.webkit.CookieManager
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

object Net {
    private const val TAG = "CratarrNet"
    /** How long a refusal holds the requests back. */
    private const val REFUSED_MS = 10 * 60_000L

    /** Per address of Cratarr's (decision 0043: a sign-in is asked for on each): the session refused there, and when.
     *  Another session (the page signed in again) is tried at once. */
    private val refusals = java.util.concurrent.ConcurrentHashMap<String, Pair<String, Long>>()

    private fun ours(prefs: Prefs, url: String) = Prefs.originOf(url)?.let { it in prefs.origins() } == true
    private fun session(url: String) = cookie(url).orEmpty()

    /** The WebView's cookies for [url] (its session, once signed in there); null without any, or without a WebView. */
    fun cookie(url: String): String? = try {
        CookieManager.getInstance().getCookie(url)?.takeIf { it.isNotBlank() }
    } catch (e: Exception) {
        null
    }

    /** A fingerprint of the session on the address in use (never the cookie itself): it changes as the page signs in
     *  or out there, or as another address is taken into use. */
    fun sessionMark(context: Context): String {
        val url = Prefs(context).serverUrl ?: return ""
        val cookie = "$url\n${cookie(url).orEmpty()}"
        return MessageDigest.getInstance("SHA-256").digest(cookie.toByteArray()).joinToString("") { "%02x".format(it) }
    }

    /** Whether this address of Cratarr's refused this session a short while ago (requests to it wait meanwhile). */
    fun refused(context: Context, url: String): Boolean {
        val origin = Prefs.originOf(url) ?: return false
        val (refusedSession, at) = refusals[origin] ?: return false
        return ours(Prefs(context), url) && refusedSession == session(url) && SystemClock.elapsedRealtime() - at < REFUSED_MS
    }

    /** A connection to [url], with the session where there is one; none while it would be refused again ([refused]),
     *  unless [anyway] (Settings' test, which the owner asks for). */
    fun open(context: Context, url: String, anyway: Boolean = false): HttpURLConnection {
        if (!anyway && refused(context, url)) throw IOException("waiting: Cratarr refused the session")
        val c = URL(url).openConnection() as HttpURLConnection
        cookie(url)?.let { c.setRequestProperty("Cookie", it) }
        return c
    }

    /** The answer's status, noted: a refusal from an address of Cratarr's holds its requests back, an answer lifts that. */
    fun code(context: Context, c: HttpURLConnection): Int {
        val code = c.responseCode
        val url = c.url.toString()
        val origin = Prefs.originOf(url)
        if (origin != null && ours(Prefs(context), url)) {
            if (code == 401 || code == 403) {
                if (refusals[origin]?.first != session(url)) Log.w(TAG, "$origin refused the app ($code): waiting ${REFUSED_MS / 60_000} min")
                refusals[origin] = session(url) to SystemClock.elapsedRealtime()
            } else if (code in 200..399) {
                refusals.remove(origin)
            }
        }
        return code
    }
}
