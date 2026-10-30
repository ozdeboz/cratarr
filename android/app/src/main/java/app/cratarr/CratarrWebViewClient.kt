/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's JellyfinWebViewClient (jellyfin-android, GPL-2.0), without its
 * script injection and Jellyfin sign-in. Links off the server open outside the app, and a page whose renderer died
 * comes back instead of taking the app down. Local mode's covers (decision 0052) come from the app.
 */
package app.cratarr

import android.content.ActivityNotFoundException
import android.content.Intent
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
) : WebViewClientCompat() {

    /** Local mode's covers (decision 0052): `/__device/cover/{album_id}` on Cratarr's pages, from Android; the page's
     *  own files from the app's copy of it ([BundledPage]). */
    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url
        if (Prefs.originOf(url.toString()) !in origins) return null
        val album = url.path?.takeIf { it.startsWith(DEVICE_COVER) }?.removePrefix(DEVICE_COVER)
            ?: return BundledPage.answer(view.context, request.method, url)
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
        Log.w(TAG, "the page's renderer is gone (crashed: ${detail.didCrash()})")
        onGone()
        return true
    }

    private companion object {
        const val TAG = "CratarrWeb"
        const val DEVICE_COVER = "/__device/cover/"
    }
}
