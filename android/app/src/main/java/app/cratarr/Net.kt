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

    @Volatile private var refusedAt = 0L
    /** The address and session that were refused; another one is tried at once. */
    @Volatile private var refusedFor: String? = null

    private fun session(prefs: Prefs) = "${prefs.publicAddress}\n${prefs.publicAddress?.let { cookie(it) }}"
    private fun toPublic(prefs: Prefs, url: String) = prefs.publicAddress?.let(Prefs::originOf)?.let { it == Prefs.originOf(url) } == true

    /** The WebView's cookies for [url] (its session, once signed in there); null without any, or without a WebView. */
    fun cookie(url: String): String? = try {
        CookieManager.getInstance().getCookie(url)?.takeIf { it.isNotBlank() }
    } catch (e: Exception) {
        null
    }

    /** A fingerprint of the session on the public address (never the cookie itself): it changes as the page signs in
     *  or out. */
    fun sessionMark(context: Context): String {
        val cookie = Prefs(context).publicAddress?.let { cookie(it) } ?: return ""
        return MessageDigest.getInstance("SHA-256").digest(cookie.toByteArray()).joinToString("") { "%02x".format(it) }
    }

    /** Whether the public address refused this session a short while ago (requests to it wait meanwhile). */
    fun refused(context: Context, url: String): Boolean {
        val prefs = Prefs(context)
        return toPublic(prefs, url) && refusedFor == session(prefs) && SystemClock.elapsedRealtime() - refusedAt < REFUSED_MS
    }

    /** A connection to [url], with the session where there is one; none while it would be refused again ([refused]),
     *  unless [anyway] (Settings' test, which the owner asks for). */
    fun open(context: Context, url: String, anyway: Boolean = false): HttpURLConnection {
        if (!anyway && refused(context, url)) throw IOException("waiting: Cratarr refused the session")
        val c = URL(url).openConnection() as HttpURLConnection
        cookie(url)?.let { c.setRequestProperty("Cookie", it) }
        return c
    }

    /** The answer's status, noted: a refusal from the public address holds its requests back, an answer lifts that. */
    fun code(context: Context, c: HttpURLConnection): Int {
        val code = c.responseCode
        val prefs = Prefs(context)
        if (toPublic(prefs, c.url.toString())) {
            if (code == 401 || code == 403) {
                if (refusedFor != session(prefs)) Log.w(TAG, "the public address refused the app ($code): waiting ${REFUSED_MS / 60_000} min")
                refusedFor = session(prefs)
                refusedAt = SystemClock.elapsedRealtime()
            } else if (code in 200..399) {
                refusedFor = null
            }
        }
        return code
    }
}
