package app.cratarr

/**
 * Android's output's speed over time as it actually ran (the speed it gave back when set, 0 while paused), so a place
 * it last told is counted on through each speed it had, never at the one set now for the time before it (the owner,
 * 2026-11-03: "we need to measure the tape truthfully. No guessing anywhere"; a brake from −1× counted the second
 * since the last report at its new, lower speed, saw the tape far behind its plan and steered it faster). Pure; any
 * thread.
 */
class OutputPace {
    /** Each change: from when (elapsed realtime, ns), how fast; oldest first. Before the first, still. */
    private val changes = ArrayDeque<Pair<Long, Double>>()

    /** From [atNs] it runs at [speed] (0: paused). */
    @Synchronized
    fun set(atNs: Long, speed: Double) {
        val last = changes.lastOrNull()
        if (last != null && last.second == speed) return
        // One set at the same moment as the last replaces it.
        if (last != null && last.first >= atNs) changes.removeLast()
        changes.addLast(atNs to speed)
    }

    /** How far it went (µs of what it was given) from [fromNs] to [toNs], through each speed it ran at between. */
    @Synchronized
    fun goneUs(fromNs: Long, toNs: Long): Double {
        if (toNs <= fromNs) return 0.0
        var us = 0.0
        var speed = 0.0
        var at = fromNs
        for ((t, s) in changes) {
            if (t <= fromNs) {
                speed = s
                continue
            }
            if (t >= toNs) break
            us += (t - at) / 1000.0 * speed
            at = t
            speed = s
        }
        return us + (toNs - at) / 1000.0 * speed
    }

    /** How fast it ran at [ns] (0 before the first change). */
    @Synchronized
    fun at(ns: Long): Double = changes.lastOrNull { it.first <= ns }?.second ?: 0.0

    /** The changes before [ns] let go but the one in force then: no place is counted from before it again. */
    @Synchronized
    fun forget(ns: Long) {
        while (changes.size >= 2 && changes.elementAt(1).first <= ns) changes.removeFirst()
    }
}
