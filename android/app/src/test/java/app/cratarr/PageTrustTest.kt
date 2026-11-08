package app.cratarr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The page open in the WebView keeps talking to the app with Cratarr out of reach, the page it trusted as it loaded (0.16.0
 * on the tablet, 2026-11-08: offline, every message of the open page was dropped, so the queue's taps played nothing); a page
 * loaded while it is not trusted is not.
 */
class PageTrustTest {
    private val home = "http://192.168.178.5:8081"
    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")

    @Test
    fun the_page_trusted_as_it_loaded_is_heard_with_cratarr_gone_and_a_page_loaded_then_is_not() {
        PageTrust.loading(home, trustedNow = true)
        assertTrue("trusted now", PageTrust.trusts(home, trustedNow = true))
        assertTrue("Cratarr out of reach: the page open is the one it trusted", PageTrust.trusts(home, trustedNow = false))
        assertFalse("another origin is not that page", PageTrust.trusts("http://192.168.178.6:8081", trustedNow = false))
        PageTrust.loading(home, trustedNow = false)
        assertFalse("a page loaded while not trusted is not heard", PageTrust.trusts(home, trustedNow = false))
        assertTrue(PageTrust.trusts(home, trustedNow = true))
        PageTrust.loading(null, trustedNow = true)
        assertFalse(PageTrust.trusts(home, trustedNow = false))
    }

    @Test
    fun the_app_hears_the_page_by_its_trust_and_judges_each_load_as_it_starts() {
        val activity = source("MainActivity.kt")
        assertTrue(activity.contains("PageTrust.trusts(origin, Reach.trusts(this, origin))"))
        assertFalse("never by Reach alone", activity.contains("data != null && Reach.trusts("))
        val started = source("CratarrWebViewClient.kt").substringAfter("override fun onPageStarted(").substringBefore("override fun onPageFinished(")
        assertTrue("the app's own copy, or trusted now", started.contains("PageTrust.loading(origin, origin != null && (fromCopy || Reach.trusts(view.context, origin)))"))
        assertTrue("whether it is the app's copy is known first", started.indexOf("fromCopy = ") < started.indexOf("PageTrust.loading("))
        // The page's service worker asks the network first, which in the app is the copy: a load so answered is the copy's too,
        // as when the app opens with Cratarr away.
        assertTrue(activity.contains("BundledPage.answer(app, request.method, request.url)?.also { client.copyAnswered(request.method, request.url) }"))
        val copied = source("CratarrWebViewClient.kt").substringAfter("fun copyAnswered(").substringBefore("override fun onPageStarted(")
        assertTrue("a page, not a file of it", copied.contains("!path.substringAfterLast('/').contains('.')") && copied.contains("bundled = withoutRoute(url.toString())"))
    }
}
