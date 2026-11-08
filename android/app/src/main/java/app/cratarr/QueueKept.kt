package app.cratarr

import org.json.JSONException
import org.json.JSONObject

/**
 * A queue as kept on the device ([Prefs.savedQueue], decision 0101, points 14 and 19). Cratarr holds its own copy of Cratarr's
 * mode's queue, and gives it back as the player connects with its word on whether it plays on, so the copy here goes into the
 * player only where that cannot be: Cratarr out of reach, or a queue that went on while it was ([AHEAD]), which it cannot know
 * of. Then the player's next report is how Play On takes it up. Nothing here knows which player plays: ExoPlayer now, the tape
 * engine's too.
 */
object QueueKept {
    /** In a kept queue: Cratarr has not been told how it stands. */
    const val AHEAD = "ahead"

    private fun read(saved: String): JSONObject? = try {
        JSONObject(saved)
    } catch (e: JSONException) {
        null
    }

    /** Whether [saved] holds any track. */
    fun holds(saved: String): Boolean = (read(saved)?.optJSONArray("tracks")?.length() ?: 0) > 0

    /** Whether the player starts on [saved] in Cratarr's mode, before Cratarr has been asked: its copy is the older. */
    fun atStart(saved: String): Boolean = holds(saved) && read(saved)?.optBoolean(AHEAD) == true

    /** Whether a player holding [holding] tracks in Cratarr's mode takes up [saved], Cratarr out of reach for [who]. Nobody
     *  signed in has no Cratarr. */
    fun whenAway(saved: String, holding: Int, who: String): Boolean = holding == 0 && who != Who.NONE && holds(saved)

    /** What the player holds is ahead of Cratarr, by how its reports went: the last one [through], the queue in one that was
     *  ([queueSent]). */
    fun aheadNow(through: Boolean, queueSent: Boolean): Boolean = !(through && queueSent)
}
