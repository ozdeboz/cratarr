package app.cratarr

import android.content.Context
import android.net.Uri
import android.webkit.WebResourceResponse
import org.json.JSONObject

/**
 * The app's own copy of the page (decision 0085, step 1; 0.11): the web app's build (web/dist), packed in the APK's
 * assets under `page/`, answered in place of the server's page files on Cratarr's own addresses. Cookies, the session
 * and every address stay as they are, and the page opens with Cratarr out of reach. The API, the streams, the sign-in
 * and the guest page still go to Cratarr.
 *
 * Which page goes by the two builds (each one's `build.json`: the release and its build): Cratarr's release newer, its
 * page (this app is the older); older, the app's; the same release and build, the app's; the same release built again,
 * Cratarr's, as the fresher; Cratarr not heard from yet, or out of reach, the app's.
 */
object BundledPage {
    private const val DIR = "page"
    /** Cratarr's own, never the page's: its API, Subsonic's, the sign-in, the guest page, the device's covers. */
    private val CRATARRS = listOf("/api", "/rest", "/signin", "/signout", "/guest", "/__device")

    /** A page's build: its release (X.Y.Z) and its build (its main script's hash). */
    data class Build(val version: String, val build: String)

    @Volatile private var own: Build? = null
    @Volatile private var files: Set<String>? = null

    /** The app's own page's build; null where the app was built without one. */
    fun own(context: Context): Build? {
        files(context)
        return own
    }

    /** The files of the app's own page, read once (with its build). */
    private fun files(context: Context): Set<String> = files ?: synchronized(this) {
        files ?: run {
            val found = mutableSetOf<String>()
            fun walk(dir: String) {
                val names = try { context.assets.list(dir) } catch (e: Exception) { null } ?: return
                for (n in names) {
                    val p = "$dir/$n"
                    // A name without a dot may be a folder: Android lists a file as having no entries of its own.
                    if (context.assets.list(p).isNullOrEmpty()) found += p.removePrefix("$DIR/") else walk(p)
                }
            }
            walk(DIR)
            own = if ("build.json" in found) {
                try { context.assets.open("$DIR/build.json").use { parse(it.readBytes().decodeToString()) } } catch (e: Exception) { null }
            } else null
            found.also { files = it }
        }
    }

    /** A `build.json` (or the health check's `web`); null when it says no release or no build. */
    fun parse(json: String): Build? = try { of(JSONObject(json)) } catch (e: Exception) { null }

    fun of(o: JSONObject?): Build? {
        if (o == null || o.isNull("version") || o.isNull("build")) return null
        return Build(o.optString("version").ifEmpty { return null }, o.optString("build").ifEmpty { return null })
    }

    /** Cratarr's page as its health check told it (`web`), kept for when Cratarr cannot be reached. */
    fun told(context: Context, web: Build?) {
        val prefs = Prefs(context)
        val kept = web?.let { "${it.version} ${it.build}" }
        if (kept != prefs.serverPage) prefs.serverPage = kept
    }

    /** Whether the app's own page is served (above). */
    fun serves(context: Context): Boolean {
        val mine = own(context) ?: return false
        val theirs = Prefs(context).serverPage?.split(' ')?.takeIf { it.size == 2 }?.let { Build(it[0], it[1]) } ?: return true
        return prefer(mine, theirs)
    }

    /** The app's page over Cratarr's [theirs] (above). */
    fun prefer(mine: Build, theirs: Build): Boolean {
        val c = compare(theirs.version, mine.version)
        return c < 0 || (c == 0 && theirs.build == mine.build)
    }

    /** Two releases (X.Y.Z) in order: negative when [a] is the older. */
    fun compare(a: String, b: String): Int {
        val x = a.split('.').map { it.toIntOrNull() ?: 0 }
        val y = b.split('.').map { it.toIntOrNull() ?: 0 }
        for (i in 0 until maxOf(x.size, y.size)) {
            val d = x.getOrElse(i) { 0 }.compareTo(y.getOrElse(i) { 0 })
            if (d != 0) return d
        }
        return 0
    }

    /** The answer from the app's own page for a request on Cratarr's address; null: Cratarr answers. As the server
     *  does: a file of the page as it is, any other address without a dot the page itself (its own routes). */
    fun answer(context: Context, method: String, url: Uri): WebResourceResponse? {
        if (method != "GET") return null
        val path = url.path ?: "/"
        if (CRATARRS.any { path == it || path.startsWith("$it/") }) return null
        if (!serves(context)) return null
        val p = path.trimStart('/')
        val file = when {
            p.isEmpty() -> "index.html"
            p in files(context) -> p
            '.' !in p -> "index.html"
            else -> return null
        }
        val stream = try { context.assets.open("$DIR/$file") } catch (e: Exception) { return null }
        // As the server's: Vite's fingerprinted files forever, the rest asked again each time.
        val cache = if (file.startsWith("assets/")) "public, max-age=31536000, immutable" else "no-cache"
        val type = typeOf(file)
        val text = type.startsWith("text/") || type.endsWith("json") || type.endsWith("javascript") || type.endsWith("xml")
        return WebResourceResponse(type, if (text) "utf-8" else null, 200, "OK", mapOf("Cache-Control" to cache), stream)
    }

    private fun typeOf(file: String): String = when (file.substringAfterLast('.', "").lowercase()) {
        "html" -> "text/html"
        "js", "mjs" -> "text/javascript"
        "css" -> "text/css"
        "json" -> "application/json"
        "webmanifest" -> "application/manifest+json"
        "svg" -> "image/svg+xml"
        "png" -> "image/png"
        "jpg", "jpeg" -> "image/jpeg"
        "webp" -> "image/webp"
        "avif" -> "image/avif"
        "ico" -> "image/x-icon"
        "woff2" -> "font/woff2"
        "woff" -> "font/woff"
        "wasm" -> "application/wasm"
        "txt" -> "text/plain"
        else -> "application/octet-stream"
    }
}
