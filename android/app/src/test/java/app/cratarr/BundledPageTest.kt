package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which page the app serves (decision 0085, step 1: 632's rule, 2026-10-30). */
class BundledPageTest {
    private val mine = BundledPage.Build("0.11.0", "abc")

    @Test
    fun theNewerReleaseWins() {
        assertFalse("Cratarr newer: its page", BundledPage.prefer(mine, BundledPage.Build("0.11.1", "abc")))
        assertFalse(BundledPage.prefer(mine, BundledPage.Build("0.12.0", "zzz")))
        assertTrue("Cratarr older: the app's", BundledPage.prefer(mine, BundledPage.Build("0.10.9", "abc")))
        assertTrue("numbers, not letters", BundledPage.prefer(BundledPage.Build("0.10.0", "x"), BundledPage.Build("0.9.18", "y")))
    }

    @Test
    fun theSameReleaseGoesByItsBuild() {
        assertTrue("the same build: the app's", BundledPage.prefer(mine, BundledPage.Build("0.11.0", "abc")))
        assertFalse("built again: Cratarr's, the fresher", BundledPage.prefer(mine, BundledPage.Build("0.11.0", "def")))
    }

    @Test
    fun aBuildIsReadOrNone() {
        assertEquals(mine, BundledPage.parse("""{"version":"0.11.0","build":"abc"}"""))
        assertNull("no release", BundledPage.parse("""{"version":null,"build":"abc"}"""))
        assertNull(BundledPage.parse("""{"build":"abc"}"""))
        assertNull(BundledPage.parse("not json"))
        assertEquals(0, BundledPage.compare("1.2", "1.2.0"))
    }
}
