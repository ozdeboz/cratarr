package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ExportPathsTest {
    @Test
    fun a_path_inside_the_folder_splits_into_its_folders_and_its_file() {
        assertEquals(listOf("Miles Davis", "Kind of Blue") to "01 So What.mp3", ExportPaths.parts("Miles Davis/Kind of Blue/01 So What.mp3"))
        assertEquals(emptyList<String>() to "Kind of Blue.m3u", ExportPaths.parts("Kind of Blue.m3u"))
        // Dots inside a name are a name's.
        assertEquals(listOf("A.B.") to "..mp3", ExportPaths.parts("A.B./..mp3"))
    }

    @Test
    fun a_path_that_leaves_the_folder_or_names_nothing_is_refused() {
        for (p in listOf("", "/etc/passwd", "../x.mp3", "a/../../x.mp3", "a/./x.mp3", "a//x.mp3", "a/", "a\\..\\x.mp3", "..", ".")) {
            assertNull(p, ExportPaths.parts(p))
        }
    }

    @Test
    fun the_type_goes_by_the_extension() {
        assertEquals("audio/mpeg", ExportPaths.typeOf("01 So What.mp3"))
        assertEquals("audio/flac", ExportPaths.typeOf("x.FLAC"))
        assertEquals("audio/ogg", ExportPaths.typeOf("x.opus"))
        assertEquals("audio/ogg", ExportPaths.typeOf("x.ogg"))
        assertEquals("audio/mp4", ExportPaths.typeOf("x.m4a"))
        assertEquals("audio/wav", ExportPaths.typeOf("x.wav"))
        assertEquals("audio/aiff", ExportPaths.typeOf("x.aif"))
        assertEquals("audio/aiff", ExportPaths.typeOf("x.aiff"))
        assertEquals("audio/x-mpegurl", ExportPaths.typeOf("Kind of Blue.m3u"))
        assertEquals("text/plain", ExportPaths.typeOf("notes.txt"))
        assertEquals(ExportPaths.ANY_TYPE, ExportPaths.typeOf("cover.jpg.part"))
        assertEquals(ExportPaths.ANY_TYPE, ExportPaths.typeOf("README"))
    }

    @Test
    fun a_file_is_created_with_a_type_that_keeps_its_name() {
        val types = mapOf("mp3" to "audio/mpeg", "wav" to "audio/x-wav", "ogg" to "audio/ogg")
        val extensions = mapOf("audio/mpeg" to "mp3", "audio/x-wav" to "wav", "audio/ogg" to "ogg")
        fun create(name: String) = ExportPaths.createType(name, types::get, extensions::get)
        // The device agrees: ours.
        assertEquals("audio/mpeg", create("a.mp3"))
        // The device names the extension another type: the device's.
        assertEquals("audio/x-wav", create("a.wav"))
        // The device does not know the extension (ours would make it a.opus.ogg): none in particular.
        assertEquals(ExportPaths.ANY_TYPE, create("a.opus"))
        assertEquals(ExportPaths.ANY_TYPE, create("README"))
    }

    @Test
    fun the_file_url_encodes_the_path_slashes_and_all() {
        assertEquals(
            "http://nas:8081/api/export/t0k-en/file?path=Miles%20Davis%2FKind%20of%20Blue%2F01%20So%20What.mp3",
            ExportPaths.fileUrl("http://nas:8081/", "t0k-en", "Miles Davis/Kind of Blue/01 So What.mp3"),
        )
        assertEquals(
            "https://c.example/api/export/t/file?path=Bj%C3%B6rk%2FPost%2F01%20Army%20of%20Me%20%26%20%2B%3F%23.flac",
            ExportPaths.fileUrl("https://c.example", "t", "Björk/Post/01 Army of Me & +?#.flac"),
        )
        // As Uri.encode: these stay.
        assertEquals("a-b_c.d!e~f'g(h)i*", ExportPaths.encode("a-b_c.d!e~f'g(h)i*"))
    }
}
