package app.cratarr

/**
 * Whether the page in the WebView may talk to the app (`window.CratarrApp`): as [Reach.trusts] says now, or, for the page
 * loaded while it did, until another page loads. The home address over plain http is trusted only while it answers as the
 * app's own Cratarr on this network, so with Cratarr out of reach the page already open, the very one it trusted, had every
 * message dropped: play, a tap in the queue, where the controls point (0.16.0 on the tablet, 2026-11-08: offline the queue's
 * taps moved the page's mark and nothing played). Nothing new reaches an open page but by loading another, and each load is
 * judged as it starts ([CratarrWebViewClient.onPageStarted]): trusted where Reach trusts its origin then, or where it is the
 * app's own copy of the page ([BundledPage]), the app's own code whatever the network, as when it opens with Cratarr away.
 */
object PageTrust {
    /** The origin of the page loading or loaded, where it was trusted as it started; null otherwise. */
    @Volatile private var loaded: String? = null

    /** A page starts loading at [origin], trusted then or not ([trustedNow]). */
    fun loading(origin: String?, trustedNow: Boolean) {
        loaded = origin?.takeIf { trustedNow }
    }

    /** Whether a message from the page at [origin] is taken: trusted now ([trustedNow]), or the page trusted as it loaded. */
    fun trusts(origin: String, trustedNow: Boolean): Boolean = trustedNow || origin == loaded
}
