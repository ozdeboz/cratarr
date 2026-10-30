/*
 * Cratarr's sign-in (decision 0039): away from home the reverse proxy lets through only a signed-in device's requests,
 * signed in on Cratarr's own sign-in page, which the app shows in its WebView like any other page. The session is the
 * WebView's cookie, and the app's own requests (streams, downloads, Play On) carry it too; the app keeps no password.
 * A refusal (401: not signed in, or the session ended; 403: not let in) holds the app's requests to the public address
 * back for a while, until the session changes (the page signed in again): asking again and again would get the phone
 * banned as a prober (it was, by CrowdSec, 2026-10-16).
 */
package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
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
    const val REFUSED_MS = 10 * 60_000L

    /** Per address of Cratarr's (decision 0043: a sign-in is asked for on each): the session refused there, and when.
     *  Another session (the page signed in again) is tried at once. */
    private val refusals = java.util.concurrent.ConcurrentHashMap<String, Pair<String, Long>>()

    /** The id of Play On's connection now (decision 0045): it goes with each of the app's own requests to Cratarr
     *  (`X-Cratarr-Connection`), so Cratarr's log ties them to that connection. */
    @Volatile var connection: String? = null

    /** Whether the connection now is paid for by the byte (mobile data, a phone's hotspot): streams then take
     *  Cratarr's smaller copies (`Downloads.locate`). */
    fun metered(context: Context): Boolean =
        (context.getSystemService(Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager).isActiveNetworkMetered

    private fun ours(prefs: Prefs, url: String) = Prefs.originOf(url)?.let { it in prefs.origins() } == true

    /** Whether [url]'s address refused this session within [REFUSED_MS] (0.9.7: the page says the local address did,
     *  as the app goes by the public one meanwhile). */
    fun refused(url: String): Boolean {
        val origin = Prefs.originOf(url) ?: return false
        val (was, at) = refusals[origin] ?: return false
        return was == session(url) && SystemClock.elapsedRealtime() - at < REFUSED_MS
    }
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

    /** The app wrote another session to this address ([Sessions], 0.12: one sign-in for both addresses, the owner
     *  2026-10-30): its requests go again at once rather than after the wait, and the ask to sign in goes. Refused
     *  again, the wait and the ask come back. */
    fun forgive(context: Context, url: String) {
        val origin = Prefs.originOf(url) ?: return
        if (refusals.remove(origin) != null) signedIn(context)
    }

    /** Until when (epoch ms, the device's clock) requests to this address of Cratarr's wait after a refusal; null: they
     *  do not. */
    fun heldUntil(context: Context, url: String): Long? {
        if (!refused(context, url)) return null
        val at = refusals[Prefs.originOf(url) ?: return null]?.second ?: return null
        return System.currentTimeMillis() + (REFUSED_MS - (SystemClock.elapsedRealtime() - at))
    }

    /** A connection to [url], with the session where there is one; none while it would be refused again ([refused]),
     *  unless [anyway] (Settings' test, which the owner asks for). */
    fun open(context: Context, url: String, anyway: Boolean = false): HttpURLConnection {
        if (!anyway && refused(context, url)) throw IOException("waiting: Cratarr refused the session")
        val c = URL(url).openConnection() as HttpURLConnection
        cookie(url)?.let { c.setRequestProperty("Cookie", it) }
        connection?.let { if (ours(Prefs(context), url)) c.setRequestProperty("X-Cratarr-Connection", it) }
        return c
    }

    /** The answer's status, noted: a refusal from an address of Cratarr's holds its requests back, an answer lifts that. */
    fun code(context: Context, c: HttpURLConnection): Int {
        val code = c.responseCode
        val url = c.url.toString()
        val origin = Prefs.originOf(url)
        if (origin != null && ours(Prefs(context), url)) {
            if (code == 401 || code == 403) {
                if (refusals[origin]?.first != session(url)) {
                    Log.w(TAG, "$origin refused the app ($code): waiting ${REFUSED_MS / 60_000} min")
                    LinkLog.add(
                        context, "warn", "signin", "refused",
                        "address" to origin, "status" to code, "path" to c.url.path, "wait_min" to REFUSED_MS / 60_000, "session" to (cookie(url) != null),
                    )
                    askSignIn(context, origin)
                }
                refusals[origin] = session(url) to SystemClock.elapsedRealtime()
            } else if (code in 200..399) {
                if (refusals.remove(origin) != null) signedIn(context)
            }
        }
        return code
    }

    private const val SIGN_IN_CHANNEL = "signin"
    private const val SIGN_IN_NOTIFICATION = 9

    /** Refused: the owner is asked to sign in there, as the music may have stopped with nothing on the screen to say
     *  why (the phone away from home with no session on the public address, 2026-10-17). The page signs in; the wait
     *  above ends as the session changes. */
    private fun askSignIn(context: Context, origin: String) {
        val app = context.applicationContext
        val notes = app.getSystemService(NotificationManager::class.java) ?: return
        notes.createNotificationChannel(NotificationChannel(SIGN_IN_CHANNEL, app.getString(R.string.sign_in), NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(
            app,
            0,
            Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val note = Notification.Builder(app, SIGN_IN_CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(app.getString(R.string.sign_in))
            .setContentText(origin)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        try {
            notes.notify(SIGN_IN_NOTIFICATION, note)
        } catch (e: SecurityException) {
            // Notifications not allowed: the log line above is all there is.
        }
    }

    /** Let in again: the ask goes. */
    private fun signedIn(context: Context) {
        context.applicationContext.getSystemService(NotificationManager::class.java)?.cancel(SIGN_IN_NOTIFICATION)
    }
}
