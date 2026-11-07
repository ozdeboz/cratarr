/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's JellyfinWebViewClient (jellyfin-android, GPL-2.0), without its
 * script injection and Jellyfin sign-in. Links off the server open outside the app, and a page whose renderer died
 * comes back instead of taking the app down. Local mode's covers (decision 0052) come from the app.
 */
package app.cratarr

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.net.http.SslError
import android.util.Log
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import androidx.webkit.WebResourceErrorCompat
import androidx.webkit.WebViewClientCompat
import androidx.webkit.WebViewFeature
import java.io.ByteArrayInputStream

class CratarrWebViewClient(
    /** Cratarr's origins (its two addresses'): pages there stay in the app. */
    private val origins: Set<String>,
    /** The page itself cannot be loaded (with why). */
    private val onError: (String) -> Unit,
    /** The page's renderer is gone: the WebView must be made anew. */
    private val onGone: () -> Unit,
    /** An album's cover of the device's own music, as JPEG (null: none); asked for off the main thread. */
    private val deviceCover: (String) -> ByteArray?,
    /** A server's cover (decision 0098): its server's id, its cover's, the size; its type and bytes, null for none. */
    private val serverCover: (String, String, Int) -> Pair<String, ByteArray>? = { _, _, _ -> null },
    /** What is kept of the library, by its address, for the page to read ([DeviceIndex.answer]); asked for off the main thread. */
    private val deviceIndex: (String) -> WebResourceResponse? = { null },
    /** A page of Cratarr's finished loading (its URL): the session follows it ([Sessions.loaded]). */
    private val onLoaded: (WebView, String) -> Unit = { _, _ -> },
) : WebViewClientCompat() {

    /** The page the app's own copy just answered as the main frame (its address without the #route), taken by the load
     *  it starts ([onPageStarted]); whether the load under way is one; and the last page loaded again, once
     *  ([onPageFinished]). */
    @Volatile private var bundled: String? = null
    private var fromCopy = false
    private var reloaded: String? = null

    /** Local mode's covers (decision 0052): `/__device/cover/{album_id}` on Cratarr's pages, from Android; a server's
     *  (decision 0098) `/__server/cover/{server}/{cover}?size=`, fetched by the app with the sign-in it keeps; the page's
     *  own files from the app's copy of it ([BundledPage]). */
    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url
        if (Prefs.originOf(url.toString()) !in origins) return null
        // The page's Sign out (a form posted to /signout, which its service worker leaves alone): both addresses' session
        // ends once it has loaded (0.12: one sign-in for both addresses, the owner 2026-10-30). Not cleared here: the
        // request still carries the session, so Cratarr ends it.
        if (request.isForMainFrame && url.path == Sessions.SIGN_OUT) Sessions.signOutAsked()
        url.path?.let(deviceIndex)?.let { return it }
        if (url.path == DeviceAnswers.PAGE_PATH) return DeviceAnswers.answer(view.context, url.getQueryParameter("ask"))
        url.path?.takeIf { it.startsWith(SERVER_COVER) }?.removePrefix(SERVER_COVER)?.let { rest ->
            val parts = rest.split('/', limit = 2)
            val got = if (parts.size == 2) serverCover(Uri.decode(parts[0]), Uri.decode(parts[1]), url.getQueryParameter("size")?.toIntOrNull() ?: 600) else null
            return got?.let { (type, bytes) -> WebResourceResponse(type, null, 200, "OK", mapOf("Cache-Control" to "max-age=86400"), ByteArrayInputStream(bytes)) }
                ?: WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
        }
        val album = url.path?.takeIf { it.startsWith(DEVICE_COVER) }?.removePrefix(DEVICE_COVER)
            ?: return BundledPage.answer(view.context, request.method, url)?.also {
                if (request.isForMainFrame) bundled = withoutRoute(url.toString())
            }
        val jpeg = deviceCover(album) ?: return WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
        return WebResourceResponse("image/jpeg", null, 200, "OK", mapOf("Cache-Control" to "max-age=86400"), ByteArrayInputStream(jpeg))
    }

    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        if (!request.isForMainFrame || Prefs.originOf(request.url.toString()) in origins) return false
        // Cratarr's page sent on elsewhere (its address moved, `http://` to `https://` say): not a link to open outside,
        // which would leave the app blank. The app looks again where Cratarr is.
        if (request.isRedirect) {
            Log.w(TAG, "sent on to ${request.url}")
            onError(view.context.getString(R.string.sent_on, request.url.toString()))
            return true
        }
        try {
            view.context.startActivity(Intent(Intent.ACTION_VIEW, request.url))
        } catch (e: ActivityNotFoundException) {
            Log.w(TAG, "nothing opens ${request.url}")
        }
        return true
    }

    override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
        super.doUpdateVisitedHistory(view, url, isReload)
        DeviceWatch.page = DeviceWatch.pageOf(url)
    }

    override fun onPageStarted(view: WebView, url: String, favicon: android.graphics.Bitmap?) {
        super.onPageStarted(view, url, favicon)
        fromCopy = bundled == withoutRoute(url)
        bundled = null
    }

    override fun onPageFinished(view: WebView, url: String) {
        super.onPageFinished(view, url)
        if (Prefs.originOf(url) !in origins) return
        // A page reached through Cratarr's redirect (the sign-in's, back to the page) is never asked of this client
        // (WebView does not intercept a redirect's target), so it came from Cratarr while the app's own copy is the one
        // to show (seen on the tablet, 2026-11-04: Cratarr's older page after signing in): loaded again, once, it comes
        // from the app's copy.
        val page = withoutRoute(url)
        val path = Uri.parse(url).path ?: "/"
        if (BundledPage.pagePath(path) && !fromCopy && reloaded != page && BundledPage.serves(view.context)) {
            reloaded = page
            view.reload()
            return
        }
        if (fromCopy) reloaded = null
        onLoaded(view, url)
    }

    private fun withoutRoute(url: String) = url.substringBefore('#')

    override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceErrorCompat) {
        if (!request.isForMainFrame) return
        val why = if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_RESOURCE_ERROR_GET_DESCRIPTION)) error.description.toString() else ""
        Log.w(TAG, "cannot load ${request.url}: $why")
        onError(why)
    }

    override fun onReceivedHttpError(view: WebView, request: WebResourceRequest, errorResponse: WebResourceResponse) {
        if (!request.isForMainFrame) return
        Log.w(TAG, "HTTP ${errorResponse.statusCode} for ${request.url}")
        // The sign-in page's own answers (0.9.7: a wrong password is 401, a sign-in from another site 403), each a page
        // that says why: the page's to show, never covered by the app's "cannot reach".
        val path = request.url.path.orEmpty()
        if ((path == "/signin" || path == "/signout") && errorResponse.mimeType.orEmpty().startsWith("text/html")) return
        onError("HTTP ${errorResponse.statusCode}")
    }

    override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
        Log.w(TAG, "certificate refused: $error")
        handler.cancel()
        onError("certificate refused")
    }

    override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
        DeviceWatch.rendererGone(view.context, detail.didCrash(), detail.rendererPriorityAtExit())
        onGone()
        return true
    }

    private companion object {
        const val TAG = "CratarrWeb"
        const val DEVICE_COVER = "/__device/cover/"
        const val SERVER_COVER = "/__server/cover/"
    }
}
