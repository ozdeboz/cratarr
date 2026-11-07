package app.cratarr

import app.cratarr.Sessions.Step
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

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
    fun theSignInPageSaysNobodyIsSignedIn() {
        assertTrue(Sessions.nobodyAfter(Sessions.SIGN_IN, Step.NOTHING))
        assertTrue("signed out into it", Sessions.nobodyAfter(Sessions.SIGN_IN, Step.CLEAR))
        assertFalse("the other address's session taken: it goes on as that one", Sessions.nobodyAfter(Sessions.SIGN_IN, Step.TAKE))
        assertFalse("any other page", Sessions.nobodyAfter("/", Step.NOTHING))
        assertFalse(Sessions.nobodyAfter(Sessions.SIGN_OUT, Step.NOTHING))
    }

    // What runs only on a device, read from its source: says a line is there, not that it works.

    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")

    @Test
    fun theSignInPageMakesItNobodysHereUnlessTheOtherSessionWasTaken() {
        val loaded = source("Sessions.kt").substringAfter("fun loaded(").substringBefore("\n    }\n")
        assertTrue("if (nobodyAfter(path, step)) Who.set(context, null)" in loaded)
    }

    @Test
    fun theAppIsNobodysWhenThePageHearsNoOneIsSignedInAndStays() {
        val heard = source("MainActivity.kt").substringAfter("\"nobody\" ->").substringBefore("\n")
        assertTrue(heard, "Who.set(this@MainActivity, null)" in heard)
    }

    @Test
    fun theFirstPageWaitsForWhoseItIsAMomentAtMost() {
        val start = source("MainActivity.kt").substringAfter("val looking = ").substringBefore("show(it)")
        assertTrue("Plays.look(this@MainActivity)" in start)
        assertTrue("within the first look's limit", start.indexOf("Plays.look") < start.indexOf("withTimeoutOrNull(FIRST_LOOK_MS)"))
    }

    @Test
    fun signingOutInTheAppEndsTheSessionOnBothAddresses() {
        val signOut = source("MainActivity.kt").substringAfter("\"sign-out\" -> {").substringBefore("\"account-downloads-remove\"")
        assertTrue("Sessions.signOutAsked()" in signOut)
        assertTrue("before the page is told to end it", signOut.indexOf("Sessions.signOutAsked()") < signOut.indexOf("signed-out"))
    }

    @Test
    fun aSignOutIsKnownBeforeAnythingOfItCanWait() {
        val main = source("MainActivity.kt")
        val signOut = main.substringAfter("\"sign-out\" -> {")
        assertTrue("the flush alone takes seconds, and answers land in them", signOut.indexOf("Sessions.signOutAsked()") < signOut.indexOf("Plays.flush"))
        val account = main.substringAfter("\"account\" -> {").substringBefore("\"nobody\" ->")
        assertTrue("the page's own answer, on its way across the bridge", "Sessions.signingOut" in account)
        val learn = source("Plays.kt").substringAfter("private fun learn(").substringBefore("\n    }\n")
        assertTrue("noted before asking", learn.indexOf("Who.current(app)") < learn.indexOf("Net.open"))
        assertTrue("stands(asked, Who.current(app), Sessions.signingOut)" in learn)
    }

    @Test
    fun thePageOpensAsTheAppsAccountOnEitherAddress() {
        val keep = source("MainActivity.kt").substringAfter("private fun keepPagePrefs(").substringBefore("\n    }\n")
        assertTrue("localStorage.setItem('cratarr-who', \${JSONObject.quote(Who.forPage(who))})" in keep)
        assertFalse("nothing kept yet is no reason to say nothing of whose it is", "?: return" in keep)
    }

    @Test
    fun whoseThingsTheseAreDoesNotGoToAnotherPhone() {
        assertTrue("android:allowBackup=\"false\"" in File("src/main/AndroidManifest.xml").readText())
    }

    @Test
    fun aMoveBetweenAddressesIsNoSignOutForTheOneUserOfAServerWithSignInOff() {
        val signedIn = source("Plays.kt").substringAfter("fun signedIn(").substringBefore("\n    }\n")
        assertTrue("if (Who.current(app) != Who.keyOf(\"\")) Who.set(app, null)" in signedIn)
        assertEquals("@", Who.keyOf(""))
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
