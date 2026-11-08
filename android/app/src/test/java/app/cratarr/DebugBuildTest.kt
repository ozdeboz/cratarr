package app.cratarr

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** What only a debug build does, held by its source: the app's page readable with DevTools over adb (D22), never in a release. */
class DebugBuildTest {
    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")

    @Test
    fun a_debug_builds_page_can_be_read_with_devtools_and_a_releases_cannot() {
        assertTrue(source("MainActivity.kt").contains("if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)"))
    }
}
