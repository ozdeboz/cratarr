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

    /** Whether a Cratarr answers at [url] (its health check) within [ms]. Blocking. */
    fun answers(url: String, ms: Int = PUBLIC_MS): Boolean = try {
        val c = URL("${url.trimEnd('/')}/api/health").openConnection() as HttpURLConnection
        c.connectTimeout = ms
        c.readTimeout = ms
        val ok = c.responseCode == 200
        c.disconnect()
        ok
    } catch (e: Exception) {
        false
    }

    /** Looks again which address answers, and uses it; the address in use after. Blocking. */
    fun check(context: Context): String? {
        val prefs = Prefs(context)
        val local = prefs.preferredLocal
        val public = prefs.publicAddress
        val pick = if (local == null || public == null || local == public) {
            prefs.serverUrl
        } else {
            // Both at once: away, the local one's silence and the public one's answer take no longer than the first.
            val l = asks.submit<Boolean> { answers(local, LOCAL_MS) }
            val r = asks.submit<Boolean> { answers(public, PUBLIC_MS) }
            when {
                l.get() -> local
                r.get() -> public
                else -> prefs.serverUrl
            }
        }
        checked = SystemClock.elapsedRealtime()
        // Written down whenever it differs (an address just changed, too): who follows the address in use hears it.
        if (pick != prefs.inUse) {
            Log.i(TAG, "now on $pick")
            prefs.use(pick)
        }
        return pick
    }

    /** Settings' "Test both connections": whether each address answers (null: not set). Blocking. */
    fun test(context: Context): Pair<Boolean?, Boolean?> {
        val prefs = Prefs(context)
        val local = prefs.localAddress?.let { a -> asks.submit<Boolean> { answers(a, PUBLIC_MS) } }
        val public = prefs.publicAddress?.let { a -> asks.submit<Boolean> { answers(a, PUBLIC_MS) } }
        return local?.get() to public?.get()
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
