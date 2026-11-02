package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Whose the pending plays are (0.13; the owner, 2026-11-02): only the account signed in's are told. */
class PlaysTest {
    private fun play(id: String, account: String? = null) =
        JSONObject().put("id", id).put("at", 1L).apply { account?.let { put("account", it) } }

    @Test
    fun theAccountIsWhatApiMeSays() {
        assertEquals("ann", Plays.accountOf(200, """{"name":"ann","admin":false}"""))
        assertEquals("one user", "", Plays.accountOf(404, """{"error":"Signing in is off: there is one user."}"""))
        assertNull("no one signed in", Plays.accountOf(404, """{"error":"No one is signed in."}"""))
        assertNull(Plays.accountOf(401, null))
        assertNull(Plays.accountOf(200, "not json"))
    }

    @Test
    fun onlyTheAccountSignedInsAreTold() {
        assertTrue(Plays.ours(play("a", "ann"), "ann"))
        assertFalse("kept for bob, not told as ann's", Plays.ours(play("b", "bob"), "ann"))
        assertTrue(Plays.ours(play("c", ""), ""))
        assertFalse(Plays.ours(play("c", ""), "ann"))
        assertFalse("none yet: no one's", Plays.ours(play("d"), "ann"))
    }

    @Test
    fun thoseCarryingNoAccountTakeTheOneKnown() {
        val was = listOf(play("a"), play("b", "bob"))
        val now = Plays.stamped(was, "ann")
        assertEquals(listOf("ann", "bob"), now.map { it.getString("account") })
        assertEquals(listOf("a", "b"), now.map { it.getString("id") })
        assertFalse("a copy", was[0].has("account"))
    }
}
