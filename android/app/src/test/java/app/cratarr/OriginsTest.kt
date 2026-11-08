package app.cratarr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The origins given to WebView, which throws on a rule it does not take: a stored one would end every start. */
class OriginsTest {
    @Test
    fun http_and_https_keep_their_origin() {
        assertEquals("http://192.168.1.5:8081", Prefs.origin("http", "192.168.1.5", 8081))
        assertEquals("https://cratarr.example.org", Prefs.origin("https", "cratarr.example.org", -1))
        assertEquals("as typed, in small letters", "http://nas.local:8081", Prefs.origin("HTTP", "NAS.Local", 8081))
        assertEquals("http://nas_box", Prefs.origin("http", "nas_box", -1))
        assertEquals("http://my-nas.", Prefs.origin("http", "my-nas.", -1))
        assertEquals("http://[fd00::1]:8081", Prefs.origin("http", "[FD00::1]", 8081))
        assertEquals("http://[::ffff:192.168.1.5]", Prefs.origin("http", "[::ffff:192.168.1.5]", -1))
        assertEquals("a host in another script", "https://bücher.example", Prefs.origin("https", "bücher.example", -1))
    }

    @Test
    fun a_misspelt_or_other_scheme_has_no_origin() {
        assertNull(Prefs.origin("htp", "192.168.1.5", 8081))
        assertNull(Prefs.origin("smb", "nas", -1))
        assertNull(Prefs.origin("ftp", "nas", -1))
        assertNull(Prefs.origin("content", "media", -1))
        assertNull(Prefs.origin(null, "nas", -1))
    }

    @Test
    fun a_host_webview_cannot_take_has_no_origin() {
        assertNull(Prefs.origin("http", null, -1))
        assertNull(Prefs.origin("http", "", 8081))
        assertNull("a space", Prefs.origin("http", "my nas", 8081))
        assertNull("a port not read as one", Prefs.origin("http", "nas:abc", -1))
        assertNull("a part left empty", Prefs.origin("http", "nas..local", -1))
        assertNull(Prefs.origin("http", ".", -1))
        assertNull("too long", Prefs.origin("http", "a.".repeat(127) + "a", -1))
        assertNull("brackets not closed", Prefs.origin("http", "[fd00::1", -1))
        assertNull("no address in them", Prefs.origin("http", "[]", -1))
        assertNull("not an address", Prefs.origin("http", "[nas]", -1))
        assertNull("a zone", Prefs.origin("http", "[fe80::1%wlan0]", -1))
    }

    @Test
    fun a_pattern_is_no_address_and_opens_no_more_than_one_origin() {
        assertNull(Prefs.origin("https", "*.example.org", -1))
        assertNull(Prefs.origin("https", "*", -1))
    }

    @Test
    fun a_port_there_cannot_be_has_no_origin() {
        assertNull(Prefs.origin("http", "nas", 0))
        assertNull(Prefs.origin("http", "nas", 65536))
        assertNull(Prefs.origin("http", "nas", -2))
        assertEquals("http://nas:65535", Prefs.origin("http", "nas", 65535))
        assertEquals("http://nas:1", Prefs.origin("http", "nas", 1))
    }

    @Test
    fun a_rule_webview_refuses_is_dropped_and_the_others_stay() {
        val refused = "http://refused"
        val good = setOf("https://cratarr.example.org", "http://192.168.1.5:8081")
        val taken = Prefs.withoutRefused(good + refused) { rules ->
            if (refused in rules) throw IllegalArgumentException("not a rule") else rules
        }
        assertEquals(good, taken)
    }

    @Test
    fun two_rules_webview_refuses_leave_the_one_it_takes() {
        val good = "https://cratarr.example.org"
        assertEquals(setOf(good), Prefs.withoutRefused(setOf("http://a", good, "http://b")) { rules ->
            if (rules != setOf(good)) throw IllegalArgumentException("not a rule") else rules
        })
    }

    @Test
    fun rules_webview_takes_are_given_to_it_once_and_as_they_are() {
        val calls = mutableListOf<Set<String>>()
        val rules = setOf("https://cratarr.example.org", "http://192.168.1.5:8081")
        assertEquals("taken", Prefs.withoutRefused(rules) { calls.add(it); "taken" })
        assertEquals(listOf(rules), calls)
    }

    @Test
    fun no_rule_webview_takes_is_no_listener_and_no_crash() {
        assertNull(Prefs.withoutRefused<Any>(setOf("http://a", "http://b", "http://c")) { throw IllegalArgumentException("not a rule") })
    }

    @Test
    fun no_rules_at_all_is_not_asked_of_webview() {
        assertNull(Prefs.withoutRefused<Any>(emptySet()) { throw IllegalArgumentException("no rules") })
    }
}
