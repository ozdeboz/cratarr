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
    fun anAnswerStandsOnlyIfNothingMovedWhileItWasAsked() {
        assertTrue(Plays.stands("@ann", "@ann", false))
        assertTrue("the first to sign in here", Plays.stands("-", "-", false))
        assertFalse("signed out while it was asked", Plays.stands("@ann", "-", false))
        assertFalse("and another come", Plays.stands("@ann", "@bo", false))
        assertFalse("asked with nobody set, her session standing until the page ends it", Plays.stands("-", "-", true))
    }

    @Test
    fun nobodySignedInIsAnAnswerNotASilence() {
        val cratarrs = "Cookie realm=\"Cratarr\", form-action=\"/signin\", cookie-name=\"cratarr-session\""
        assertTrue(Plays.signedOut(401, null, cratarrs))
        assertFalse("a bare one is something on the way's", Plays.signedOut(401, null, null))
        assertFalse("and so is another's realm", Plays.signedOut(401, null, "Basic realm=\"proxy\""))
        assertTrue(Plays.signedOut(404, """{"error":"No one is signed in."}""", null))
        assertFalse("one user", Plays.signedOut(404, """{"error":"Signing in is off: there is one user."}""", null))
        assertFalse(Plays.signedOut(200, """{"name":"ann"}""", null))
        assertFalse("not reached", Plays.signedOut(502, null, null))
        assertFalse("the proxy's refusal, away from home: the account's things stay", Plays.signedOut(403, null, null))
    }
}
