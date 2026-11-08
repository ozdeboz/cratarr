/*
 * What this device answers for itself (PREVIEW2's D18 to D20): crates, Sounds like and what comes next, by the rules crate over what
 * [DeviceIndex] keeps, so Autoplay runs with no page open and the page asks the same answer here ([PAGE_PATH]). Kotlin decides nothing:
 * it hands in what is kept, the plays not told yet and what plays here, and does what it is told.
 * One copy, opened when first asked and let go as soon as Cratarr answers any of the app's requests again ([Net.code]): the tablet held
 * 173 MB of it, and a second in the WebView is what it could not hold. So a page in the app never opens its own.
 */
package app.cratarr

import android.content.Context
import android.util.Log
import android.webkit.WebResourceResponse
import app.cratarr.rules.RulesDevice
import app.cratarr.rules.deviceOpen
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayInputStream
import kotlin.random.Random

object DeviceAnswers {
    private const val TAG = "CratarrAnswers"

    /** Where the page asks (web/src/lib/deviceAnswers). */
    const val PAGE_PATH = "/__device/answer"

    private class Held(val who: String, val stands: List<Long?>, val device: RulesDevice)
    @Volatile private var held: Held? = null

    /** The device, opened anew where the account or anything kept moved; null where nothing is kept, or no rules library is here. */
    @Synchronized
    private fun device(app: Context): RulesDevice? {
        val who = Who.current(app)
        val stands = DeviceIndex.stands(app)
        held?.takeIf { it.who == who && it.stands == stands }?.let { return it.device }
        release()
        val index = DeviceIndex.kept(app, "index") ?: return null
        val device = try {
            deviceOpen(index, DeviceIndex.kept(app, "sounds"), DeviceIndex.kept(app, "own"))
        } catch (e: LinkageError) {
            Log.w(TAG, "no rules library: $e")
            null
        } ?: return null
        held = Held(who, stands, device)
        return device
    }

    /** Lets it go: its memory goes with it. Told at every answer from Cratarr, so with nothing held it costs nothing. */
    fun release() {
        if (held == null) return
        synchronized(this) {
            held?.device?.close()
            held = null
        }
    }

    /** The answer to what is asked, this device's unsent plays gone by; null where it has none. */
    @Synchronized
    private fun ask(context: Context, asked: String): String? {
        val app = context.applicationContext
        val started = System.nanoTime()
        val before = held?.device
        val device = device(app) ?: return null
        device.ask(JSONObject().put("ask", "plays").put("plays", Plays.waiting(app)).toString())
        val said = device.ask(asked)
        timed(app, asked, (System.nanoTime() - started) / 1_000_000, device !== before)
        return said
    }

    /** One line in the link log per answer (D22's measures): what was asked, how long it took all told, and whether the device was
     *  opened for it (`cold`), so a tablet's own numbers can be read from the file with no DevTools. */
    private fun timed(app: Context, asked: String, ms: Long, cold: Boolean) {
        val kind = runCatching { JSONObject(asked).optString("ask") }.getOrNull()?.takeIf { it.isNotEmpty() } ?: "?"
        LinkLog.add(app, "info", "answers", "answered", "kind" to kind, "ms" to ms, "cold" to cold)
    }

    /** What comes next among what plays here, as Cratarr's answer has it; null where nothing is kept to answer from. */
    fun pick(context: Context, asked: JSONObject, here: List<String>): JSONArray? {
        val full = JSONObject(asked.toString())
            .put("ask", "pick")
            .put("here", JSONArray(here))
            .put("draw", Random.nextLong(Long.MAX_VALUE shr 11))
            .put("now", System.currentTimeMillis() / 1000)
        return ask(context, full.toString())?.let { JSONObject(it).optJSONArray("tracks") }
    }

    /** The page's question, whole in its `ask`. */
    fun answer(context: Context, asked: String?): WebResourceResponse {
        val said = asked?.let { ask(context, it) }
            ?: return WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
        return WebResourceResponse("application/json", "utf-8", 200, "OK", mapOf("Cache-Control" to "no-store"), ByteArrayInputStream(said.toByteArray(Charsets.UTF_8)))
    }
}
