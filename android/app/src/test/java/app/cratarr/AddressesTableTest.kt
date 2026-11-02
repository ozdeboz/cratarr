package app.cratarr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/** Where the app opens the page after a sign-in, against the one table of the page's addresses (shared/addresses.json,
 *  2026-11-02), as the server and the page check it too. */
class AddressesTableTest {
    private val links = JSONObject(File("../../shared/addresses.json").readText()).getJSONObject("links")

    private fun link(name: String, service: String): String {
        val l = links.getJSONObject(name)
        val view = l.getString("view").replace("{service}", service).replace(":", "%3A").replace("/", "%2F")
        return "#/${l.getString("page")}?view=$view"
    }

    @Test
    fun aSignInComesBackWhereTheTableSays() {
        assertEquals(link("listening_service", "lastfm"), connectedAt("lastfm", null))
        assertEquals(link("spotify_account", ""), connectedAt("spotify", null))
    }
}
