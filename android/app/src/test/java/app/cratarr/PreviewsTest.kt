package app.cratarr

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PreviewsTest {
    @Test
    fun a_bandcamp_preview_is_told_by_its_key_and_nothing_else_is() {
        assertTrue(Previews.isPreview("bandcamp/bc.MXxodHRwczovL2EuYmFuZGNhbXAuY29tL2FsYnVtL2I"))
        assertFalse(Previews.isPreview("cratarr/tr-1"))
        assertFalse(Previews.isPreview("bandcampish/1"))
        assertFalse(Previews.isPreview("local/bandcamp/1"))
    }
}
