package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Test

class ExportPathsTest {
    @Test
    fun the_type_goes_by_the_extension() {
        assertEquals("application/zip", ExportPaths.typeOf("Kind of Blue.zip"))
        assertEquals("audio/mpeg", ExportPaths.typeOf("01 So What.mp3"))
        assertEquals("audio/flac", ExportPaths.typeOf("x.FLAC"))
        assertEquals("audio/ogg", ExportPaths.typeOf("x.opus"))
        assertEquals("audio/ogg", ExportPaths.typeOf("x.ogg"))
        assertEquals("audio/mp4", ExportPaths.typeOf("x.m4a"))
        assertEquals("audio/wav", ExportPaths.typeOf("x.wav"))
        assertEquals("audio/aiff", ExportPaths.typeOf("x.aif"))
        assertEquals("audio/aiff", ExportPaths.typeOf("x.aiff"))
        assertEquals("audio/x-mpegurl", ExportPaths.typeOf("Kind of Blue.m3u"))
        assertEquals("text/plain", ExportPaths.typeOf("Missing tracks.txt"))
        assertEquals(ExportPaths.ANY_TYPE, ExportPaths.typeOf("cover.jpg.part"))
        assertEquals(ExportPaths.ANY_TYPE, ExportPaths.typeOf("README"))
    }

    @Test
    fun a_file_is_saved_with_a_type_that_keeps_its_name() {
        val types = mapOf("mp3" to "audio/mpeg", "wav" to "audio/x-wav", "ogg" to "audio/ogg", "zip" to "application/zip")
        val extensions = mapOf("audio/mpeg" to "mp3", "audio/x-wav" to "wav", "audio/ogg" to "ogg", "application/zip" to "zip")
        fun create(name: String) = ExportPaths.createType(name, types::get, extensions::get)
        // The device agrees: ours.
        assertEquals("audio/mpeg", create("01 So What.mp3"))
        assertEquals("application/zip", create("Kind of Blue.zip"))
        // The device names the extension another type: the device's.
        assertEquals("audio/x-wav", create("a.wav"))
        // The device does not know the extension (ours would make it a.opus.ogg): none in particular.
        assertEquals(ExportPaths.ANY_TYPE, create("a.opus"))
        assertEquals(ExportPaths.ANY_TYPE, create("README"))
    }

    @Test
    fun the_download_is_the_prepared_exports_address() {
        assertEquals("http://nas:8081/api/export/0f3a9c", ExportPaths.url("http://nas:8081/", "0f3a9c"))
        assertEquals("https://c.example/api/export/a%2Fb%3F", ExportPaths.url("https://c.example", "a/b?"))
        // As Uri.encode: these stay.
        assertEquals("a-b_c.d!e~f'g(h)i*", ExportPaths.encode("a-b_c.d!e~f'g(h)i*"))
    }
}
