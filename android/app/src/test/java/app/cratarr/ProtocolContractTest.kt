package app.cratarr

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The app against the wire's one description of a player command (the audit, 2026-10-29: one protocol): the page's
 * types generated from the server's (web/src/lib/generated/PlayerCommand.ts, ts-rs). What the app makes of the page's
 * calls is a command the server knows, with fields it knows; what the server sends a player, the app handles.
 */
class ProtocolContractTest {
    /** Each command's name and its fields, as the generated type says (comments left out). */
    private val wire: Map<String, Set<String>> = run {
        val text = File("../../web/src/lib/generated/PlayerCommand.ts").readText().replace(Regex("""/\*\*.*?\*/""", RegexOption.DOT_MATCHES_ALL), "")
        Regex("""\{ "command": "([a-z_]+)"([^}]*)\}""").findAll(text).associate { m ->
            m.groupValues[1] to Regex("""([a-z_]+)\??:""").findAll(m.groupValues[2]).map { it.groupValues[1] }.toSet()
        }
    }

    /** Commands the server carries out itself (on its own queue, or as a play or upcoming it then sends). */
    private val serversOwn = setOf("clear", "handover", "join", "leave", "queue", "queue_delete", "queue_rename", "restore", "shuffle")

    @Test
    fun theGeneratedTypeIsRead() {
        assertTrue("commands read: ${wire.keys}", wire.size >= 30)
        assertEquals(setOf("tracks", "index", "position_ms", "shuffle", "paused", "queue_id", "queue_name", "playlist", "album", "favourites", "library", "limit", "name", "radio"), wire.getValue("play"))
    }

    @Test
    fun whatTheAppMakesOfThePagesCallsIsACommandTheServerKnows() {
        val tracks = JSONArray().put(JSONObject().put("source", "cratarr").put("id", "tr-1"))
        val calls = listOf(
            JSONObject().put("op", "queue").put("tracks", tracks).put("index", 0).put("positionMs", 0).put("play", true).put("shuffled", false).put("name", "A"),
            JSONObject().put("op", "play"),
            JSONObject().put("op", "seek").put("positionMs", 1_000).put("wind", true),
            JSONObject().put("op", "sleep").put("atMs", 5_000).put("endOfTrack", false),
            JSONObject().put("op", "add").put("tracks", tracks).put("at", 1),
            JSONObject().put("op", "move").put("from", 0).put("to", 1),
            JSONObject().put("op", "remove").put("index", 0),
            JSONObject().put("op", "upcoming").put("tracks", tracks).put("shuffled", true),
            JSONObject().put("op", "jump").put("index", 0),
            JSONObject().put("op", "repeat").put("mode", "all"),
            JSONObject().put("op", "radio").put("on", true),
            JSONObject().put("op", "autoplay").put("mode", "similar"),
            JSONObject().put("op", "skip_favourites").put("on", true),
            JSONObject().put("op", "direction").put("dir", "L"),
            JSONObject().put("op", "wind").put("dir", "back").put("phase", "start"),
        ) + listOf("next", "previous", "pause", "toggle", "stop").map { JSONObject().put("op", it) }
        for (call in calls) {
            val c = PageCommand.of(call.put("type", "player")) ?: error("no command for ${call.optString("op")}")
            val name = c.getString("command")
            val known = wire[name] ?: error("${call.optString("op")} became $name, which the server does not know")
            val fields = c.keys().asSequence().filter { it != "command" }.toSet()
            assertTrue("${call.optString("op")} → $name: ${fields - known} not on the wire", known.containsAll(fields))
        }
    }

    @Test
    fun everyCommandAPlayerIsSentIsHandled() {
        val cases = listOf("PlayerCommands.kt", "PlayOnLink.kt")
            .map { File("src/main/java/app/cratarr/$it").readText() }
            .flatMap { Regex("""^\s+((?:"[a-z_]+"(?:, )?)+) ->""", RegexOption.MULTILINE).findAll(it).flatMap { m -> Regex("\"([a-z_]+)\"").findAll(m.groupValues[1]).map { q -> q.groupValues[1] } }.toList() }
            .toSet() + "sample"
        val unhandled = wire.keys - serversOwn - cases
        assertTrue("sent to players but not handled in the app: $unhandled", unhandled.isEmpty())
    }
}
