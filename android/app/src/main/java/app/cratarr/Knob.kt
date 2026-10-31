package app.cratarr

/**
 * The tape deck's direction knob and the holds against it (0.12.4: pure directions, the owner 2026-10-31: "continuous
 * hold winding should never alter the direction from what it was before initiating it. Directions should be pure"):
 * what [StartStop] does at each of those moments, said once, as the page's lib/player/knob says it (web/tests/
 * tape-knob.test.ts and KnobTest hold both to the same cases). The knob is the setting, `L` backwards, `R` forwards: only
 * the user's turn of it changes it, never a hold, a skip, a late stream or a backwards play that could not start; a new
 * queue alone starts at R. The deck's L and R are the way the tape goes (the page draws them from the deck's speed). Pure.
 */
object Knob {
    const val L = "L"
    const val R = "R"

    const val FORWARDS = "forwards"
    const val BACKWARDS = "backwards"
    const val REST = "rest"

    /** Another track (Next, Previous, a jump in the queue) plays the way the knob is set: backwards from its end at L. */
    fun skipWay(want: String): String = if (want == L) BACKWARDS else FORWARDS

    /** Where a track played backwards from its end starts (ms): this much before it, as the page's. */
    const val FROM_END_MS = 1_500L

    /** The place a skip at L stands at in a track of [length] ms before it plays backwards; null where its length is not
     *  known or it is too short (it plays forwards then, the knob left at L). */
    fun fromEnd(length: Long?): Long? = length?.takeIf { it > FROM_END_MS }?.minus(FROM_END_MS)

    /**
     * Where a hold let go brakes to (× the track's): the way the knob is set at 1×, a hold that way braking to it on the
     * same way; one against it to a stand, the knob's way taking over from there. A hold from playing backwards with the
     * knob still at L ([backTo]): backwards is the knob's way; otherwise forwards. [dir]: the hold's way, 1 forward, -1
     * back.
     */
    fun letGoTo(dir: Int, backTo: Boolean, want: String): Int {
        val back = backTo && want == L
        return if ((dir < 0) == back) 1 else 0
    }

    /** A hold let go, its brake run out: on the way it was going at 1× backwards ([BACKWARDS]), forwards from there, or
     *  resting there ([REST]: a pause pressed meanwhile). [after]: `play` or `pause`. */
    fun afterLetGo(@Suppress("UNUSED_PARAMETER") dir: Int, backTo: Boolean, want: String, after: String): String {
        if (after == "pause") return REST
        return if (backTo && want == L) BACKWARDS else FORWARDS
    }
}
