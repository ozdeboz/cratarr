/*
 * A track's file while Cratarr is out of reach for a moment (it restarts in about 15 s; its proxy answers 502
 * meanwhile, or the connection is refused or cut): the player asks for it again every few seconds, for about a minute,
 * before it gives up on that track. Twice a restart left a player stopped (the phone at 54, the tablet at 57, 2026-10-17):
 * the player had given up after its three quick tries, then skipped on through tracks it could not load either.
 * What is gone (a 4xx) is given up at once; anything else (a device without a network to ask on: no such host) gets
 * the player's own three quick tries, so offline it goes on to what it holds on the device, as before.
 */
package app.cratarr

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import java.net.NoRouteToHostException
import java.net.UnknownHostException

@OptIn(UnstableApi::class)
class Patience : DefaultLoadErrorHandlingPolicy() {
    override fun getRetryDelayMsFor(info: LoadErrorHandlingPolicy.LoadErrorInfo): Long {
        val e = info.exception
        // Gone: no use asking again.
        if (e is HttpDataSource.InvalidResponseCodeException && e.responseCode < 500) return C.TIME_UNSET
        // Anything else: three quick tries, as the player's own rule gives.
        if (!waitFor(e)) return if (info.errorCount <= QUICK_TRIES) super.getRetryDelayMsFor(info) else C.TIME_UNSET
        return if (info.errorCount <= TRIES) minOf(info.errorCount * 2_000L, 5_000L) else C.TIME_UNSET
    }

    /** As many tries before the player hears of the failure: the waiting above, not three quick ones. */
    override fun getMinimumLoadableRetryCount(dataType: Int): Int = TRIES

    companion object {
        /** 2 + 4 + 5 s each after: about a minute in all. */
        private const val TRIES = 14
        private const val QUICK_TRIES = 3

        /** Cratarr's refusal of the session (sign in again): every track would be refused alike. */
        fun refused(e: Throwable?): Boolean = e is HttpDataSource.InvalidResponseCodeException && (e.responseCode == 401 || e.responseCode == 403)

        /** Whether this failure is Cratarr out of reach for a moment (worth waiting for), not a track that is gone or a
         *  device that is offline. */
        fun waitFor(e: Throwable?): Boolean = when (e) {
            is HttpDataSource.CleartextNotPermittedException -> false
            is HttpDataSource.InvalidResponseCodeException -> e.responseCode >= 500
            is HttpDataSource.HttpDataSourceException -> e.cause !is UnknownHostException && e.cause !is NoRouteToHostException
            else -> false
        }
    }
}
