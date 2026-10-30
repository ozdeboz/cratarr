/*
 * One sign-in for both of Cratarr's addresses (0.12: one sign-in for both addresses, the owner 2026-10-30). The WebView
 * keeps its cookies per address, so the home address and the public one each had a session of their own: taken into
 * use without one (the tablet's home address), the app was refused there, held back 10 minutes ([Net.refused]) and went
 * back to the other, or showed the sign-in page for a moment. Now the session goes with the app: the one a page last
 * loaded with is written to the other address too, under that address's own cookie (`__Host-cratarr-session` over
 * https, `cratarr-session` over plain http, as Cratarr sets them: signin.rs); a sign-in page with the other address's
 * session at hand takes that one; a sign-out ends both.
 *
 * Only between the app's own two addresses, and only while Reach trusts both ([Reach.trusts]: the home address over
 * plain http only while it answers as this app's Cratarr on this network). The cost, accepted: the public session's
 * token then also travels over the home address's plain http on the home network, as the home address's own session
 * already does, and only to the address Reach has checked is the same Cratarr.
 */
package app.cratarr

import android.content.Context
import android.net.Uri
import android.util.Log
import android.webkit.CookieManager
import java.util.concurrent.ConcurrentHashMap

object Sessions {
    private const val TAG = "CratarrSessions"
    /** Cratarr's session cookie over https (secure, path `/`, no domain: the `__Host-` prefix's rules), and over http. */
    const val HTTPS_NAME = "__Host-cratarr-session"
    const val HTTP_NAME = "cratarr-session"
    /** As long as Cratarr's own cookie lasts (sessions.rs `LIFETIME`); the session itself ends when Cratarr says. */
    const val MAX_AGE = 400L * 24 * 3600
    const val SIGN_IN = "/signin"
    const val SIGN_OUT = "/signout"

    /** What a page's load means for the session ([afterLoad]). */
    enum class Step {
        /** Nothing to do. */
        NOTHING,
        /** The session here goes to the other address too: it just loaded a page, so it is the good one. */
        SHARE,
        /** The sign-in page, with another session at the other address: that one here, and the page again. */
        TAKE,
        /** Signed out: neither address keeps a session. */
        CLEAR,
    }

    /** The session token in a `Cookie` header's worth of cookies ([CookieManager.getCookie]): the `__Host-` one first,
     *  as Cratarr reads it (signin.rs `token_of`); only a token as Cratarr makes them (64 hex digits). */
    fun tokenIn(cookies: String?): String? {
        var plain: String? = null
        for (pair in cookies.orEmpty().split(';')) {
            val at = pair.indexOf('=')
            if (at < 0) continue
            val name = pair.substring(0, at).trim()
            val token = pair.substring(at + 1).trim()
            if (token.length != 64 || !token.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) continue
            if (name == HTTPS_NAME) return token
            if (name == HTTP_NAME && plain == null) plain = token
        }
        return plain
    }

    private fun secure(origin: String) = origin.startsWith("https://", ignoreCase = true)

    /** The session cookie's name at [origin]: Cratarr's own for its scheme. */
    fun nameFor(origin: String): String = if (secure(origin)) HTTPS_NAME else HTTP_NAME

    /** [token] as [origin]'s session cookie, with the attributes Cratarr gives it there. */
    fun cookieFor(origin: String, token: String): String =
        if (secure(origin)) "$HTTPS_NAME=$token; Path=/; Max-Age=$MAX_AGE; Secure; HttpOnly; SameSite=Lax"
        else "$HTTP_NAME=$token; Path=/; Max-Age=$MAX_AGE; HttpOnly; SameSite=Lax"

    /** [origin]'s session cookie gone: expired, with the same attributes (a `__Host-` cookie is only replaced so). */
    fun clearFor(origin: String): String =
        if (secure(origin)) "$HTTPS_NAME=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Lax"
        else "$HTTP_NAME=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"

    /** What a page loaded at [path] means, with [here] the session on its address, [there] the other address's, [tried]
     *  the other's session already taken here once for the sign-in page (never twice: no loop), and [signingOut] when
     *  the page just signed out. */
    fun afterLoad(path: String, here: String?, there: String?, tried: String?, signingOut: Boolean): Step = when {
        signingOut -> Step.CLEAR
        path == SIGN_OUT -> Step.NOTHING
        path == SIGN_IN -> if (there != null && there != here && there != tried) Step.TAKE else Step.NOTHING
        here != null && here != there -> Step.SHARE
        else -> Step.NOTHING
    }

    /** Whether an address about to be used ([here] its session) should take the other's ([there]) first. */
    fun lacking(here: String?, there: String?): Boolean = here == null && there != null

    /** The page asked to sign out ([signOutAsked]): its next load ends the session on both addresses. */
    @Volatile private var signingOut = false
    /** Per address: the other's session taken for its sign-in page, so the same one is never taken twice. */
    private val tried = ConcurrentHashMap<String, String>()

    /** The page is signing out ([SIGN_OUT] asked for; Cratarr ends the session it carries). Any thread. */

    fun signOutAsked() {
        signingOut = true
    }

    /** The session token on [origin] (one of the app's addresses); null without one, or without a WebView. */
    fun token(origin: String): String? = tokenIn(Net.cookie(origin))

    /** The app's other address than [origin] (an origin of its own); null with only one. */
    private fun other(prefs: Prefs, origin: String): String? {
        val origins = prefs.origins()
        return if (origin in origins) origins.singleOrNull { it != origin } else null
    }

    /** [from]'s session written to [to], under [to]'s own cookie; both must be the app's addresses and trusted. Any
     *  hold on [to] after a refusal ends ([Net.forgive]): the session there is another now. Whether it was written. */
    fun copy(context: Context, from: String, to: String): Boolean {
        val origins = Prefs(context).origins()
        if (from == to || from !in origins || to !in origins) return false
        if (!Reach.trusts(context, from) || !Reach.trusts(context, to)) return false
        val token = token(from) ?: return false
        if (token(to) == token) return false
        return try {
            val cookies = CookieManager.getInstance()
            cookies.setCookie(to, cookieFor(to, token))
            cookies.flush()
            Net.forgive(context, to)
            Log.i(TAG, "the session from $from to $to")
            LinkLog.add(context, "info", "signin", "session shared", "from" to from, "to" to to)
            true
        } catch (e: Exception) {
            Log.w(TAG, "the session could not go to $to", e)
            false
        }
    }

    /** Signed out: the session cookie ends on both addresses. */
    fun clear(context: Context) {
        tried.clear()
        try {
            val cookies = CookieManager.getInstance()
            for (origin in Prefs(context).origins()) cookies.setCookie(origin, clearFor(origin))
            cookies.flush()
            LinkLog.add(context, "info", "signin", "signed out on both addresses")
        } catch (e: Exception) {
            Log.w(TAG, "the sessions could not be cleared", e)
        }
    }

    /** Before [url]'s address is used (its page shown, the app's requests sent there): the other address's session,
     *  when it has none. Whether one was written. */
    fun toward(context: Context, url: String): Boolean {
        val to = Prefs.originOf(url) ?: return false
        val from = other(Prefs(context), to) ?: return false
        if (!lacking(token(to), token(from))) return false
        return copy(context, from, to)
    }

    /** A page of [url] finished loading: the session followed ([afterLoad]). True: load the page again, it took the
     *  other address's session for the sign-in page (Cratarr's sign-in page sends a signed-in one on to its `next`). */
    fun loaded(context: Context, url: String): Boolean {
        val here = Prefs.originOf(url) ?: return false
        val prefs = Prefs(context)
        if (here !in prefs.origins()) return false
        val there = other(prefs, here)
        val theirs = there?.let(::token)
        val step = afterLoad(Uri.parse(url).path.orEmpty(), token(here), theirs, tried[here], signingOut)
        when (step) {
            Step.CLEAR -> {
                signingOut = false
                clear(context)
            }
            Step.SHARE -> if (there != null) copy(context, here, there)
            Step.TAKE -> if (there != null && theirs != null) {
                tried[here] = theirs
                return copy(context, there, here)
            }
            Step.NOTHING -> {}
        }
        return false
    }
}
