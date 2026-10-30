/*
 * The device's network as the page should know it (0.9.15; the owner, 2026-10-28: "the best PWA offline support
 * possible"): told as it changes, so the page knows it is out of reach without first waiting for an answer that does
 * not come. `online`: a network carries the app's traffic (a home Wi-Fi without the internet too: Cratarr may be on
 * it); `validated`: Android found the internet through it; `kind`: wifi, cellular, ethernet, vpn or other.
 */
package app.cratarr

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import org.json.JSONObject

object NetState {
    /** The network now, as the page's `network` message. */
    fun now(context: Context): JSONObject {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        return message(cm?.activeNetwork?.let { cm.getNetworkCapabilities(it) })
    }

    /** Whether any network carries the app's traffic now. */
    fun connected(context: Context): Boolean = now(context).optBoolean("online")

    private fun message(caps: NetworkCapabilities?): JSONObject {
        val online = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true
        val kind = when {
            caps == null -> "none"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
            else -> "other"
        }
        return JSONObject()
            .put("type", "network")
            .put("online", online)
            .put("validated", online && caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true)
            .put("kind", kind)
    }

    /** Follows the network the app's traffic goes over, telling [changed] (on the main thread) each time it differs. */
    class Watch(context: Context, private val changed: (JSONObject) -> Unit) {
        private val cm = context.getSystemService(ConnectivityManager::class.java)
        private val main = Handler(Looper.getMainLooper())
        private var told: String? = null
        private val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = tell(message(caps))
            override fun onLost(network: Network) = tell(message(null))
        }

        init {
            cm?.registerDefaultNetworkCallback(callback)
        }

        private fun tell(m: JSONObject) {
            main.post {
                val s = m.toString()
                if (s != told) {
                    told = s
                    changed(m)
                }
            }
        }

        fun stop() {
            try {
                cm?.unregisterNetworkCallback(callback)
            } catch (e: IllegalArgumentException) {
                // Not registered (no ConnectivityManager): nothing to stop.
            }
        }
    }
}
