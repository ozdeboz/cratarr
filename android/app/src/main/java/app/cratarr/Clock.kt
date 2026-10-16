/*
 * Sync play (decision 0037), step 1: this device's clock against Cratarr's. A timed ping (`GET /api/time`) every few
 * seconds while connected; of the last ones, the shortest round trip wins (Cratarr's time read in its middle), as
 * Snapcast does. Kept against the monotonic clock (`elapsedRealtimeNanos`), never the wall clock, which the system may
 * step at any moment. Only the request and its answer are timed: the connection is made first and kept (keep-alive),
 * so neither the TCP handshake nor a new connection sits on one side of the round trip.
 */
package app.cratarr

import android.os.SystemClock
import android.util.Log
import org.json.JSONObject

object Clock {
    /** Cratarr's time minus this device's monotonic clock (ms); NaN until measured. */
    @Volatile private var offsetMono = Double.NaN

    /** The round trip the offset came from; NaN until measured. */
    @Volatile var rttMs = Double.NaN
        private set

    /** The last pings: (round trip, offset against the monotonic clock). */
    private val pings = ArrayDeque<Pair<Double, Double>>()
    private const val KEPT = 15

    private fun mono(): Double = SystemClock.elapsedRealtimeNanos() / 1e6

    /** Cratarr's time now, by this device's clock; its wall clock until measured. */
    fun now(): Double = if (offsetMono.isNaN()) System.currentTimeMillis().toDouble() else mono() + offsetMono

    /** Cratarr's time minus this device's wall clock, for reports (0 until measured). */
    val offsetMs: Double get() = if (offsetMono.isNaN()) 0.0 else now() - System.currentTimeMillis()

    /** How the kept pings spread: the shortest and the middle round trip, and how far the offsets of the better half
     *  lie apart (ms). What Wi-Fi's power saving does shows here. */
    fun spread(): Triple<Double, Double, Double> = synchronized(pings) {
        if (pings.isEmpty()) return Triple(Double.NaN, Double.NaN, Double.NaN)
        val byRtt = pings.sortedBy { it.first }
        val better = byRtt.take((byRtt.size + 1) / 2).map { it.second }
        Triple(byRtt.first().first, byRtt[byRtt.size / 2].first, better.max() - better.min())
    }

    /** One ping, blocking (a background thread). False when Cratarr did not answer. */
    fun ping(context: android.content.Context, server: String): Boolean = try {
        val c = Net.open(context, "${server.trimEnd('/')}/api/time")
        c.connectTimeout = 3_000
        c.readTimeout = 3_000
        // A new connection's handshake before the clock starts; a kept one is reused at once.
        c.connect()
        val t0 = mono()
        // The status within the timing, as the body was (it sends the request).
        if (Net.code(context, c) !in 200..299) throw java.io.IOException("not answered")
        val text = c.inputStream.bufferedReader().use { it.readText() }
        val t1 = mono()
        // Read whole and closed (not disconnected): the connection goes back to the pool for the next ping.
        val rtt = t1 - t0
        val offset = JSONObject(text).getDouble("now_ms") - (t0 + rtt / 2)
        synchronized(pings) {
            pings.addLast(rtt to offset)
            while (pings.size > KEPT) pings.removeFirst()
            val best = pings.minBy { it.first }
            rttMs = best.first
            offsetMono = best.second
        }
        true
    } catch (e: Exception) {
        Log.w("CratarrClock", "ping: ${e.message}")
        false
    }

    /** A fresh start (another server). */
    fun forget() = synchronized(pings) {
        pings.clear()
        offsetMono = Double.NaN
        rttMs = Double.NaN
    }
}
