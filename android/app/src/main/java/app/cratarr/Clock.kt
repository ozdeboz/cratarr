/*
 * Sync play (decision 0037), step 1: this device's clock against Cratarr's. A timed ping (`GET /api/time`) every few
 * seconds while connected; of the last ones, the shortest round trip wins (Cratarr's time read in its middle), as
 * Snapcast does. Cratarr's time is then this device's wall clock plus `offsetMs`.
 */
package app.cratarr

import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

object Clock {
    /** Cratarr's time minus this device's, in ms; 0 until measured. */
    @Volatile var offsetMs = 0.0
        private set

    /** The round trip the offset came from; NaN until measured. */
    @Volatile var rttMs = Double.NaN
        private set

    /** The last pings: (round trip, offset). */
    private val pings = ArrayDeque<Pair<Double, Double>>()
    private const val KEPT = 15

    /** Cratarr's time now, by this device's clock. */
    fun now(): Double = System.currentTimeMillis() + offsetMs

    /** One ping, blocking (a background thread). False when Cratarr did not answer. */
    fun ping(server: String): Boolean = try {
        val c = URL("${server.trimEnd('/')}/api/time").openConnection() as HttpURLConnection
        c.connectTimeout = 3_000
        c.readTimeout = 3_000
        val wall = System.currentTimeMillis()
        val t0 = SystemClock.elapsedRealtimeNanos()
        val text = c.inputStream.bufferedReader().use { it.readText() }
        val t1 = SystemClock.elapsedRealtimeNanos()
        c.disconnect()
        val rtt = (t1 - t0) / 1e6
        val offset = JSONObject(text).getDouble("now_ms") - (wall + rtt / 2)
        synchronized(pings) {
            pings.addLast(rtt to offset)
            while (pings.size > KEPT) pings.removeFirst()
            val best = pings.minBy { it.first }
            rttMs = best.first
            offsetMs = best.second
        }
        true
    } catch (e: Exception) {
        Log.w("CratarrClock", "ping: ${e.message}")
        false
    }

    /** A fresh start (another server). */
    fun forget() = synchronized(pings) {
        pings.clear()
        offsetMs = 0.0
        rttMs = Double.NaN
    }
}
