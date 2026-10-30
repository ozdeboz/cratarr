/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's JellyfinWebChromeClient (jellyfin-android, GPL-2.0): the page's
 * console in the device log, new windows opened outside the app, and the file chooser.
 */
package app.cratarr

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Message
import android.util.Log
import android.webkit.ConsoleMessage
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import androidx.webkit.WebViewClientCompat

class CratarrWebChromeClient(
    private val chooseFile: (Intent, ValueCallback<Array<Uri>>) -> Unit,
) : WebChromeClient() {

    override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean {
        val level = when (consoleMessage.messageLevel()) {
            ConsoleMessage.MessageLevel.ERROR -> Log.ERROR
            ConsoleMessage.MessageLevel.WARNING -> Log.WARN
            ConsoleMessage.MessageLevel.DEBUG -> Log.DEBUG
            else -> Log.INFO
        }
        Log.println(level, "CratarrPage", "${consoleMessage.message()} (${consoleMessage.sourceId()}:${consoleMessage.lineNumber()})")
        return true
    }

    override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
        val transport = resultMsg.obj as? WebView.WebViewTransport ?: return false
        val window = WebView(view.context)
        window.webViewClient = object : WebViewClientCompat() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
                openOutside(view.context, request.url)
        }
        transport.webView = window
        resultMsg.sendToTarget()
        return true
    }

    private fun openOutside(context: Context, url: Uri): Boolean {
        // Some pages open an empty window first; there is nothing to open for that.
        if (url.toString() == "about:blank") return false
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, url))
        } catch (e: ActivityNotFoundException) {
            Log.w("CratarrWeb", "nothing opens $url")
        }
        return true
    }

    override fun onShowFileChooser(
        webView: WebView,
        filePathCallback: ValueCallback<Array<Uri>>,
        fileChooserParams: FileChooserParams?,
    ): Boolean {
        if (fileChooserParams == null) {
            filePathCallback.onReceiveValue(null)
            return true
        }
        chooseFile(fileChooserParams.createIntent(), filePathCallback)
        return true
    }
}
