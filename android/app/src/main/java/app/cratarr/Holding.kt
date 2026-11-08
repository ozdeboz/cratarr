package app.cratarr

/**
 * Holding on server loss (the owner, 2026-11-06), the app's side of the page's lib/player/holdOn: with Cratarr out of reach the
 * player stands at the place reached, the queue kept, and carries on by itself when Cratarr answers. A failure that says so
 * ([Patience.outOfReach]) never moves the queue on, so it does not run out through tracks that could not be fetched either.
 * Nothing here knows which player plays: [again] asks whichever it is to try from where it stands.
 */
class Holding(
    private val post: (Runnable, Long) -> Unit,
    private val cancel: (Runnable) -> Unit,
    private val again: () -> Unit,
) {
    private val ask = Runnable { if (now) again() }

    /** A track could not be had, Cratarr out of reach: held, or held on, and tried again in a while. */
    fun begin() {
        now = true
        cancel(ask)
        post(ask, AGAIN_MS)
    }

    /** Cratarr answers: at once, not at the next try. */
    fun answered() {
        if (!now) return
        cancel(ask)
        again()
    }

    /** It plays, or was given something else to do. */
    fun ended() {
        now = false
        cancel(ask)
    }

    companion object {
        /** Asked again after this long; a try itself takes up to a minute (Patience). */
        const val AGAIN_MS = 15_000L

        /** Held now: told as waiting, not as a failure ([PagePlayer]). */
        @Volatile var now = false
            private set
    }
}
