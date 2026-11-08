package app.cratarr

import androidx.media3.exoplayer.offline.Download
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** What the page is told of a download: "done" must mean the bytes are there, or a track shows as playing with Cratarr away and does not (Finamp's A4). */
class DownloadStateTest {
    @Test
    fun a_finished_download_with_its_bytes_there_is_done() {
        assertEquals("done", stateOf(Download.STATE_COMPLETED, 1_000, 1_000))
    }

    @Test
    fun a_finished_download_with_its_bytes_gone_reads_as_failed() {
        assertEquals("failed", stateOf(Download.STATE_COMPLETED, 0, 1_000))
    }

    @Test
    fun a_finished_download_with_some_of_its_bytes_gone_reads_as_failed() {
        assertEquals("failed", stateOf(Download.STATE_COMPLETED, 999, 1_000))
    }

    @Test
    fun what_is_not_finished_goes_by_its_own_state_whatever_the_store_holds() {
        assertEquals("failed", stateOf(Download.STATE_FAILED, 1_000, 1_000))
        assertEquals("downloading", stateOf(Download.STATE_DOWNLOADING, 10, 1_000))
        assertEquals("waiting", stateOf(Download.STATE_QUEUED, 0, 0))
        assertEquals("waiting", stateOf(Download.STATE_STOPPED, 0, 1_000))
    }

    /** The store needs a device, so what is read of it is held to the source: under the key the download wrote, for its whole length. */
    @Test
    fun the_list_asks_the_store_under_the_key_the_download_wrote() {
        val source = File("src/main/java/app/cratarr/Downloads.kt").readText()
        assertTrue(source.contains("stateOf(d.state, store.getCachedBytes(d.request.customCacheKey ?: d.request.id, 0, d.bytesDownloaded), d.bytesDownloaded)"))
    }
}
