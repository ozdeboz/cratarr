/*
 * Which of Cratarr's two addresses the app uses (as Finamp's "Prefer local network"): the local network address while
 * it is preferred and answers (nearer, and the only one a VPN that carries just the local network reaches), else the
 * public one; when neither answers, the one in use stays. Answering is tried, as Finamp does, rather than told from the
 * Wi-Fi's name as Home Assistant's app does: that needs location access, and misses a VPN. Looked at
 * when the app starts, when the network changes (Wi-Fi, mobile data, a VPN going up or down), and when the address in
 * use stops answering. Whatever was set up on the other address (a queued track, a download) is read from the one in
 * use ([rewrite]).
 */
package app.cratarr

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.Uri
import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

object Reach {
    private const val TAG = "CratarrReach"
    /** The local address answers this fast or not at all (away, its address leads nowhere). */
    private const val LOCAL_MS = 1_500
    private const val PUBLIC_MS = 5_000
    /** After a network change, this long for it to settle (a VPN comes up after the network it runs over). */
    private const val SETTLE_MS = 1_500L
    /** Failures look again at most this often. */
    private const val AGAIN_MS = 10_000L

    private val asks = Executors.newCachedThreadPool()
    private val later: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor()
    private val checking = AtomicBoolean(false)
    @Volatile private var checked = 0L
    private var watching = false

    /** A health check's answer: the address that gave it, its status (null: none), and whether Cratarr gave it (its
     *  `{"status": "ok", "playlists": n}`), or its proxy asking for a sign-in on Cratarr's sign-in page (decision 0039:
     *  401 with Cratarr's `Cookie` challenge; the page then shows the sign-in page). */
    data class Health(val at: String, val code: Int?, val cratarr: Boolean)

    /** Cratarr's health check for [url] within [ms]: answered at [url], or at the address it is sent on to for good
     *  (301, 308: `http://` typed where only `https://` is served, say). Blocking. */
    fun reach(context: Context, url: String, ms: Int = PUBLIC_MS, anyway: Boolean = false, moves: Int = 2): Health {
        val base = url.trimEnd('/')
        val c = try {
            Net.open(context, "$base/api/health", anyway)
        } catch (e: Exception) {
            return Health(url, null, false)
        }
        return try {
            // Android's own connection does not follow http to https; the WebView would, off Cratarr's addresses.
            c.instanceFollowRedirects = false
            // Not a page load: refused, it is a 401 rather than the way to the sign-in page.
            c.setRequestProperty("Accept", "application/json")
            c.connectTimeout = ms
            c.readTimeout = ms
            val code = Net.code(context, c)
            val to = c.getHeaderField("Location")?.takeIf { code == 301 || code == 308 }
                ?.let { URL(c.url, it).toString() }?.takeIf { it.endsWith("/api/health") }?.removeSuffix("/api/health")
            when {
                to != null && to != base && moves > 0 -> reach(context, to, ms, anyway, moves - 1)
                code == 200 -> Health(url, code, isCratarr(head(c)))
                else -> Health(url, code, code == 401 && c.getHeaderField("WWW-Authenticate")?.let { it.trim().startsWith("Cookie", ignoreCase = true) && "realm=\"Cratarr\"" in it } == true)
            }
        } catch (e: Exception) {
            Health(url, null, false)
        } finally {
            c.disconnect()
        }
    }

    /** The start of an answer's body: enough for Cratarr's health check, never all of some other server's page. */
    private fun head(c: HttpURLConnection): String = c.inputStream.use { s ->
        val buf = ByteArray(4_096)
        var n = 0
        while (n < buf.size) {
            val r = s.read(buf, n, buf.size - n)
            if (r < 0) break
            n += r
        }
        String(buf, 0, n)
    }

    private fun isCratarr(body: String) = try {
        JSONObject(body).let { it.optString("status") == "ok" && it.has("playlists") }
    } catch (e: Exception) {
        false
    }

    /** The status of Cratarr's health check at [url] within [ms]; null without an answer. An address of the app's that
     *  is sent on for good is kept as where it leads ([Prefs.moved]). Blocking. */
    fun status(context: Context, url: String, ms: Int = PUBLIC_MS, anyway: Boolean = false): Int? {
        val h = reach(context, url, ms, anyway)
        if (h.at != url && h.code != null) {
            Log.i(TAG, "$url moved to ${h.at}")
            Prefs(context).moved(url, h.at)
        }
        return h.code
    }

    /** Cratarr at an address as typed, as Finamp looks for its server: with the scheme typed, or else `https://` first
     *  and then `http://`. The address where Cratarr answers (null: none), and whether anything answered at all. Blocking. */
    fun find(context: Context, typed: String, ms: Int = PUBLIC_MS): Pair<String?, Boolean> {
        val t = typed.trim().trimEnd('/')
        var answered = false
        for (url in if ("://" in t) listOf(t) else listOf("https://$t", "http://$t")) {
            val h = reach(context, url, ms, anyway = true)
            if (h.cratarr) return h.at to true
            answered = answered || h.code != null
        }
        return null to answered
    }

    /** Whether a Cratarr answers at [url] (its health check) within [ms]. Blocking. */
    fun answers(context: Context, url: String, ms: Int = PUBLIC_MS): Boolean = status(context, url, ms) == 200

    /** Looks again which address answers, and uses it; the address in use after. Blocking. */
    fun check(context: Context): String? {
        val prefs = Prefs(context)
        val local = prefs.preferredLocal
        val public = prefs.publicAddress
        // The addresses as they are after asking: one that moved is kept as where it leads.
        val pick = if (local == null || public == null || local == public) {
            prefs.serverUrl?.let { status(context, it) }
            prefs.serverUrl
        } else {
            // Both at once: away, the local one's silence and the public one's answer take no longer than the first.
            val l = asks.submit<Boolean> { answers(context, local, LOCAL_MS) }
            // The proxy's refusal is an answer too: there, the page asks for the sign-in.
            val r = asks.submit<Boolean> { status(context, public, PUBLIC_MS) != null || Net.refused(context, public) }
            when {
                l.get() -> prefs.preferredLocal
                r.get() -> prefs.publicAddress
                else -> prefs.serverUrl
            }
        }
        checked = SystemClock.elapsedRealtime()
        // Written down whenever it differs (an address just changed, too): who follows the address in use hears it.
        if (pick != prefs.inUse) {
            Log.i(TAG, "now on $pick")
            LinkLog.add(context, "info", "playon", "address changed", "from" to prefs.inUse, "to" to pick)
            prefs.use(pick)
        }
        return pick
    }

    /** Settings' "Test both connections": each address's health check status (null: no answer), and whether each
     *  is set. Blocking. */
    fun test(context: Context): Triple<Boolean, Int?, Int?> {
        val prefs = Prefs(context)
        val local = prefs.localAddress?.let { a -> asks.submit<Int?> { status(context, a, PUBLIC_MS, anyway = true) } }
        val public = prefs.publicAddress?.let { a -> asks.submit<Int?> { status(context, a, PUBLIC_MS, anyway = true) } }
        return Triple(local != null, local?.get(), public?.get())
    }

    /** Looks again in the background, after [delayMs]; a failure's call only every so often. */
    fun checkSoon(context: Context, delayMs: Long = 0, failed: Boolean = false) {
        if (failed && SystemClock.elapsedRealtime() - checked < AGAIN_MS) return
        val app = context.applicationContext
        later.schedule({
            if (checking.compareAndSet(false, true)) {
                try {
                    check(app)
                } finally {
                    checking.set(false)
                }
            }
        }, delayMs, TimeUnit.MILLISECONDS)
    }

    /** Follows the network: each time another one carries the app's traffic, or it goes, which address answers. */
    @Synchronized
    fun watch(context: Context) {
        if (watching) return
        watching = true
        val app = context.applicationContext
        app.getSystemService(ConnectivityManager::class.java)?.registerDefaultNetworkCallback(
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) = checkSoon(app, SETTLE_MS)
                override fun onLost(network: Network) = checkSoon(app, SETTLE_MS)
            },
        )
    }

    /** [uri] on the address in use, when it was made for the other one (a track queued, or a download asked for, there). */
    fun rewrite(context: Context, uri: Uri): Uri {
        val prefs = Prefs(context)
        val now = prefs.serverUrl ?: return uri
        val other = listOfNotNull(prefs.localAddress, prefs.publicAddress).firstOrNull { it != now } ?: return uri
        val origin = Prefs.originOf(uri.toString()) ?: return uri
        if (origin != Prefs.originOf(other)) return uri
        val base = Uri.parse(now.trimEnd('/'))
        // The other address's own path (Cratarr under a sub-path) gives way to this one's.
        val otherPath = Uri.parse(other.trimEnd('/')).path.orEmpty()
        val rest = uri.encodedPath.orEmpty().removePrefix(otherPath)
        return uri.buildUpon()
            .scheme(base.scheme)
            .encodedAuthority(base.encodedAuthority)
            .encodedPath(base.encodedPath.orEmpty() + rest)
            .build()
    }
}
