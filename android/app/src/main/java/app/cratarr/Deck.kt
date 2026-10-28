package app.cratarr

/**
 * The tape engine's one truth (decision 0075, the app; the owner, 2026-10-28: "we do really need the robust, pure fix"):
 * where the deck is (which track of the queue, where in it) and how it moves now (its speed in track ms per ms, signed,
 * and how fast that changes, per second). Whatever reads the place (the page, Cratarr, the session, a wind's landing)
 * reads it here, worked out from the head that sounds; the page counts on from it each frame (place + v·t + ½·a·t²).
 */
data class Deck(val index: Int, val ms: Long, val speed: Double, val accel: Double, val atEnd: Boolean = false) {
    companion object {
        val STILL_AT_START = Deck(0, 0L, 0.0, 0.0)

        /**
         * [gone] ms of track from ([index], [ms]) through a queue of [lengths] (ms each, unknown as 0 or less: taken as
         * [fallback]'s), forwards where positive. Never past the queue's ends: there it stands, at its first track's
         * start or its last track's end ([Walk.edge]).
         */
        fun walk(lengths: List<Long>, index: Int, ms: Long, gone: Long, fallback: (Int) -> Long = { 0L }): Walk {
            if (lengths.isEmpty()) return Walk(0, 0L, true)
            fun len(i: Int) = lengths[i].takeIf { it > 0 } ?: fallback(i).coerceAtLeast(0L)
            var i = index.coerceIn(0, lengths.size - 1)
            var at = ms.coerceIn(0L, len(i)) + gone
            while (at > len(i)) {
                if (i == lengths.size - 1) return Walk(i, len(i), true)
                at -= len(i)
                i++
            }
            while (at < 0) {
                if (i == 0) return Walk(0, 0L, true)
                i--
                at += len(i)
            }
            return Walk(i, at, false)
        }

        /**
         * A place given by a map of other lengths (a wind stream's, from Cratarr's catalogue) made a place in the files
         * as they are ([lengths], the player's own): a place at or past its track's real end is the next track's start,
         * or the queue's last track's end ([Walk.edge]), never past it (2026-10-28, the tablet: a hold through the
         * queue landed at 234.0 s of a 233.586 s track, which ended at once, the page counting on past it).
         */
        fun settle(lengths: List<Long>, index: Int, ms: Long): Walk {
            if (lengths.isEmpty()) return Walk(0, 0L, true)
            val i = index.coerceIn(0, lengths.size - 1)
            val len = lengths[i]
            if (len <= 0 || ms < len - END_MARGIN_MS) return Walk(i, ms.coerceAtLeast(0L), false)
            if (i == lengths.size - 1) return Walk(i, len, true)
            return Walk(i + 1, 0L, false)
        }

        /** A place this near its track's real end is at its end (what is left would not be heard). */
        const val END_MARGIN_MS = 50L
    }

    /** A place in the queue; [edge]: at one of its ends, where a wind stops. */
    data class Walk(val index: Int, val ms: Long, val edge: Boolean)
}
