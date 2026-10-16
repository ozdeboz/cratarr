/*
 * Cratarr's sign-in (decision 0031, amended): away from home the reverse proxy asks for a Cratarr account (HTTP Basic,
 * the Settings → Authentication accounts). The app sends the one set in Settings → Network with each of its own
 * requests to the public address (the local network address needs none); the page's requests get it from the WebView,
 * once given (MainActivity's sign-in dialog). A refusal (401: no sign-in or a wrong one; 403: not let in) holds the
 * app's requests to the public address back for a while, until the sign-in changes: asking again and again would get
 * the phone banned as a prober (it was, by CrowdSec, 2026-10-16).
 */
package app.cratarr

import android.content.Context
import android.os.SystemClock
import android.util.Base64
import android.util.Log
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

object Net {
    private const val TAG = "CratarrNet"
    /** How long a refusal holds the requests back. */
    private const val REFUSED_MS = 10 * 60_000L

    @Volatile private var refusedAt = 0L
    /** The address and sign-in that were refused; another one is tried at once. */
    @Volatile private var refusedFor: String? = null

    private fun signIn(prefs: Prefs) = "${prefs.publicAddress}\n${prefs.signInUser}\n${prefs.signInPassword}"
    private fun toPublic(prefs: Prefs, url: String) = prefs.publicAddress?.let(Prefs::originOf)?.let { it == Prefs.originOf(url) } == true

    /** The Authorization header for [url]: when it goes to the public address and a sign-in is set. */
    fun authorization(context: Context, url: String): String? {
        val prefs = Prefs(context)
        val user = prefs.signInUser ?: return null
        val password = prefs.signInPassword ?: return null
        if (!toPublic(prefs, url)) return null
        return "Basic " + Base64.encodeToString("$user:$password".toByteArray(), Base64.NO_WRAP)
    }

    /** Whether the public address refused this sign-in a short while ago (requests to it wait meanwhile). */
    fun refused(context: Context, url: String): Boolean {
        val prefs = Prefs(context)
        return toPublic(prefs, url) && refusedFor == signIn(prefs) && SystemClock.elapsedRealtime() - refusedAt < REFUSED_MS
    }

    /** A connection to [url], signed in where it must be; none while it would be refused again ([refused]), unless
     *  [anyway] (Settings' test, which the owner asks for). */
    fun open(context: Context, url: String, anyway: Boolean = false): HttpURLConnection {
        if (!anyway && refused(context, url)) throw IOException("waiting: Cratarr's sign-in was refused")
        val c = URL(url).openConnection() as HttpURLConnection
        authorization(context, url)?.let { c.setRequestProperty("Authorization", it) }
        return c
    }

    /** The answer's status, noted: a refusal from the public address holds its requests back, an answer lifts that. */
    fun code(context: Context, c: HttpURLConnection): Int {
        val code = c.responseCode
        val prefs = Prefs(context)
        if (toPublic(prefs, c.url.toString())) {
            if (code == 401 || code == 403) {
                if (refusedFor != signIn(prefs)) Log.w(TAG, "the public address refused the app ($code): waiting ${REFUSED_MS / 60_000} min")
                refusedFor = signIn(prefs)
                refusedAt = SystemClock.elapsedRealtime()
            } else if (code in 200..399) {
                refusedFor = null
            }
        }
        return code
    }
}
