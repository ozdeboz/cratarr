package app.cratarr

import app.cratarr.Sessions.Step
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** One sign-in for both addresses (0.12, the owner 2026-10-30): the session read, written and followed. */
class SessionsTest {
    private val a = "a".repeat(64)
    private val b = "0123456789abcdef".repeat(4)
    private val home = "http://192.168.1.5:8081"
    private val away = "https://cratarr.example.org"

    @Test
    fun theTokenIsReadByEitherNameTheHostOneFirst() {
        assertEquals(a, Sessions.tokenIn("cratarr-session=$a"))
        assertEquals(a, Sessions.tokenIn("__Host-cratarr-session=$a"))
        assertEquals("as Cratarr reads it", b, Sessions.tokenIn("cratarr-session=$a; __Host-cratarr-session=$b"))
        assertEquals(a, Sessions.tokenIn("cratarr-prefs=x; cratarr-guest=${"c".repeat(32)};  cratarr-session=$a ; other=1"))
    }

    @Test
    fun anythingElseIsNoToken() {
        assertNull(Sessions.tokenIn(null))
        assertNull(Sessions.tokenIn(""))
        assertNull("a cleared one", Sessions.tokenIn("cratarr-session="))
        assertNull("not as Cratarr makes them", Sessions.tokenIn("cratarr-session=${"z".repeat(64)}"))
        assertNull(Sessions.tokenIn("cratarr-session=${"a".repeat(63)}"))
        assertNull("another cookie", Sessions.tokenIn("cratarr-guest=$a"))
        assertNull(Sessions.tokenIn("xcratarr-session=$a"))
    }

    @Test
    fun eachAddressGetsItsSchemesOwnCookie() {
        assertEquals("__Host-cratarr-session", Sessions.nameFor(away))
        assertEquals("cratarr-session", Sessions.nameFor(home))
        val secure = Sessions.cookieFor(away, a)
        assertTrue(secure.startsWith("__Host-cratarr-session=$a;"))
        for (part in listOf("Path=/", "Secure", "HttpOnly", "SameSite=Lax")) assertTrue(part, part in secure.split("; "))
        assertFalse("a __Host- cookie has none", "Domain" in secure)
        val plain = Sessions.cookieFor(home, a)
        assertTrue(plain.startsWith("cratarr-session=$a;"))
        assertFalse("no Secure over plain http", "Secure" in plain)
        assertTrue("HttpOnly" in plain.split("; ") && "Path=/" in plain.split("; "))
        assertEquals(a, Sessions.tokenIn(secure.substringBefore(';')))
        assertEquals(a, Sessions.tokenIn(plain.substringBefore(';')))
    }

    @Test
    fun aClearedCookieExpiresWithTheSameAttributes() {
        val secure = Sessions.clearFor(away)
        assertTrue(secure.startsWith("__Host-cratarr-session=;"))
        for (part in listOf("Path=/", "Max-Age=0", "Secure", "HttpOnly", "SameSite=Lax")) assertTrue(part, part in secure.split("; "))
        val plain = Sessions.clearFor(home)
        assertTrue(plain.startsWith("cratarr-session=;"))
        assertTrue("Max-Age=0" in plain.split("; "))
        assertFalse("Secure" in plain)
    }

    @Test
    fun aPageLoadedSharesItsSession() {
        assertEquals(Step.SHARE, Sessions.afterLoad("/", a, null, null, false))
        assertEquals("the last good one wins", Step.SHARE, Sessions.afterLoad("/", a, b, null, false))
        assertEquals("already the same", Step.NOTHING, Sessions.afterLoad("/", a, a, null, false))
        assertEquals("none to share", Step.NOTHING, Sessions.afterLoad("/", null, b, null, false))
    }

    @Test
    fun theSignInPageTakesTheOthersSessionOnce() {
        assertEquals(Step.TAKE, Sessions.afterLoad("/signin", null, b, null, false))
        assertEquals("a session ended here", Step.TAKE, Sessions.afterLoad("/signin", a, b, null, false))
        assertEquals("tried already: no loop", Step.NOTHING, Sessions.afterLoad("/signin", null, b, b, false))
        assertEquals("another one is tried", Step.TAKE, Sessions.afterLoad("/signin", null, b, a, false))
        assertEquals("the same one ended on both", Step.NOTHING, Sessions.afterLoad("/signin", a, a, null, false))
        assertEquals("none there", Step.NOTHING, Sessions.afterLoad("/signin", a, null, null, false))
    }

    @Test
    fun aSignOutClearsBothAndIsNeverUndone() {
        assertEquals(Step.CLEAR, Sessions.afterLoad("/signin", null, b, null, true))
        assertEquals(Step.CLEAR, Sessions.afterLoad("/signout", a, a, null, true))
        assertEquals(Step.CLEAR, Sessions.afterLoad("/", a, b, null, true))
        assertEquals("the sign-out page itself shares nothing", Step.NOTHING, Sessions.afterLoad("/signout", a, b, null, false))
    }

    @Test
    fun anAddressWithoutASessionTakesTheOthers() {
        assertTrue(Sessions.lacking(null, a))
        assertFalse(Sessions.lacking(a, b))
        assertFalse(Sessions.lacking(null, null))
        assertFalse(Sessions.lacking(a, null))
    }
}
