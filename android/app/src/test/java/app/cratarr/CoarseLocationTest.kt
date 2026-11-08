package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class CoarseLocationTest {
    private val manifest = File("src/main/AndroidManifest.xml").readText()
    private val activity = File("src/main/java/app/cratarr/MainActivity.kt").readText().replace("\r\n", "\n")

    @Test
    fun a_place_goes_out_to_one_decimal() {
        assertEquals(52.1, CoarseLocation.coarse(52.08371), 0.0)
        assertEquals(-5.4, CoarseLocation.coarse(-5.449), 0.0)
        assertEquals(0.0, CoarseLocation.coarse(0.04), 0.0)
    }

    @Test
    fun the_app_can_ask_for_a_rough_place_and_never_a_fine_one() {
        assertTrue(manifest.contains("android.permission.ACCESS_COARSE_LOCATION"))
        assertFalse(manifest.contains("ACCESS_FINE_LOCATION"))
        assertFalse(manifest.contains("ACCESS_BACKGROUND_LOCATION"))
    }

    @Test
    fun only_coarse_degrees_reach_the_page() {
        val told = activity.substringAfter("private fun tellPlace").substringBefore("\n    }\n")
        assertTrue(told, told.contains("CoarseLocation.coarse(place.latitude)"))
        assertTrue(told, told.contains("CoarseLocation.coarse(place.longitude)"))
        assertFalse(told, told.contains("\"lat\", place.latitude"))
    }

    @Test
    fun the_permission_is_asked_only_when_the_page_says_so() {
        val asked = activity.substringAfter("\"location\" ->").substringBefore("\n            \"")
        assertTrue(asked, asked.contains("locationAsk.launch"))
        assertTrue(asked, asked.contains("\"ask\""))
    }
}
