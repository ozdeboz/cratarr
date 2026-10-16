/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's JellyfinWebViewClient (jellyfin-android, GPL-2.0), without its
 * script injection and Jellyfin sign-in. Links off the server open outside the app, and a page whose renderer died
 * comes back instead of taking the app down.
 */
package app.cratarr

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.http.SslError
import android.webkit.HttpAuthHandler
import android.util.Log
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import androidx.webkit.WebResourceErrorCompat
import androidx.webkit.WebViewClientCompat
import androidx.webkit.WebViewFeature

class CratarrWebViewClient(
    /** Cratarr's origins (its two addresses'): pages there stay in the app. */
    private val origins: Set<String>,
    /** The page itself cannot be loaded (with why). */
    private val onError: (String) -> Unit,
    /** The page's renderer is gone: the WebView must be made anew. */
    private val onGone: () -> Unit,
    /** The proxy asks for a sign-in (away from home). */
    private val onSignIn: (HttpAuthHandler) -> Unit,
) : WebViewClientCompat() {

    override fun onReceivedHttpAuthRequest(view: WebView, handler: HttpAuthHandler, host: String, realm: String) = onSignIn(handler)


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
    }
}
