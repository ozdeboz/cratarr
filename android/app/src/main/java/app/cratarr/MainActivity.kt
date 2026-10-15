/*
 * Changed for Cratarr, 2026-10-15: from Jellyfin's MainActivity and WebViewFragment (jellyfin-android, GPL-2.0). One
 * WebView showing the Cratarr server's pages; no server selection, native player, downloads or casting (decision 0031).
 */
package app.cratarr

import android.Manifest
import android.app.AlertDialog
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.text.InputType
import android.util.Log
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.webkit.ValueCallback
import android.webkit.WebChromeClient.FileChooserParams
import android.webkit.WebView
import android.widget.EditText
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONException
import org.json.JSONObject

class MainActivity : AppCompatActivity() {
    private lateinit var prefs: Prefs
    private lateinit var frame: FrameLayout
    private var webView: WebView? = null

    /** The address the WebView shows. */
    private var shown: String? = null

    /** The page that said hello: where the player's reports go. */
    private var page: JavaScriptReplyProxy? = null

    /** The app's player (PlayerService), once connected; the page's player calls before that wait in `waiting`. */
    private lateinit var controllerFuture: ListenableFuture<MediaController>
    private var player: PagePlayer? = null
    private val waiting = mutableListOf<JSONObject>()
    private var dialog: AlertDialog? = null

    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private val fileChooser = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        fileCallback?.onReceiveValue(FileChooserParams.parseResult(result.resultCode, result.data))
        fileCallback = null
    }

    /** Open on Cratarr's Settings page (the launcher's Settings shortcut). */
    private var openSettings = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        openSettings = intent?.getBooleanExtra(EXTRA_SETTINGS, false) == true
        frame = FrameLayout(this)
        setContentView(frame)
        ViewCompat.setOnApplyWindowInsetsListener(frame) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    val view = webView
                    // Out of the page's history the app goes to the background, not away: the music plays on.
                    if (view != null && view.canGoBack()) view.goBack() else moveTaskToBack(true)
                }
            },
        )
        controllerFuture = MediaController.Builder(this, SessionToken(this, ComponentName(this, PlayerService::class.java))).buildAsync()
        controllerFuture.addListener(
            {
                val connected = PagePlayer(controllerFuture.get()) { message -> page?.postMessage(message) }
                player = connected
                waiting.forEach(connected::handle)
                waiting.clear()
                connected.report(withQueue = true)
            },
            ContextCompat.getMainExecutor(this),
        )
        // The position once a second while the page is in view (changes are reported as they happen; a page out of
        // view gets those only, as it may be frozen and would get them all at once when thawed).
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                player?.report(withQueue = true)
                while (true) {
                    delay(1_000)
                    player?.report()
                }
            }
        }
        addSettingsShortcut()
        askForNotifications()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.getBooleanExtra(EXTRA_SETTINGS, false)) {
            if (page != null) webView?.evaluateJavascript("location.hash = '#/settings'", null) else openSettings = true
        }
    }

    override fun onResume() {
        super.onResume()
        // Back from Android's background dialog, say: the page shows the settings as they are now.
        page?.postMessage(appMessage())
        val url = prefs.serverUrl
        when {
            url == null -> askForAddress()
            url != shown -> show(url)
        }
        PlayerService.ready(this)
    }

    override fun onDestroy() {
        webView?.let {
            frame.removeView(it)
            it.destroy()
        }
        webView = null
        player?.release()
        MediaController.releaseFuture(controllerFuture)
        super.onDestroy()
    }

    /** A new WebView on [url]: on first start, after the address changed, and after the page's renderer died. */
    private fun show(url: String) {
        dialog?.dismiss()
        webView?.let {
            frame.removeView(it)
            it.destroy()
        }
        page = null
        val origin = Prefs.originOf(url)
        val view = WebView(this)
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            // Play On starts music with nobody touching the screen.
            mediaPlaybackRequiresUserGesture = false
            setSupportMultipleWindows(true)
        }
        // The page keeps its priority when not in view: it is the player.
        view.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
        view.webViewClient = CratarrWebViewClient(origin, onError = { cannotReach(url, it) }, onGone = { show(url) })
        view.webChromeClient = CratarrWebChromeClient { intent, callback ->
            fileCallback?.onReceiveValue(null)
            fileCallback = callback
            try {
                fileChooser.launch(intent)
            } catch (e: ActivityNotFoundException) {
                fileCallback = null
                callback.onReceiveValue(null)
            }
        }
        // `window.CratarrApp`, on the server's pages only.
        if (origin != null && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(view, "CratarrApp", setOf(origin)) { _, message, _, isMainFrame, reply ->
                val data = message.data
                if (isMainFrame && data != null) onPageMessage(data, reply)
            }
        }
        frame.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        webView = view
        shown = url
        view.loadUrl(if (openSettings) "${url.trimEnd('/')}/#/settings" else url)
        openSettings = false
    }

    /** A message from the page: `hello`, `player` (its player's calls), `device` and `device-set` (this device's
     *  settings), `battery`, `address`, `open` (a link outside). */
    private fun onPageMessage(data: String, reply: JavaScriptReplyProxy) {
        val message = try {
            JSONObject(data)
        } catch (e: JSONException) {
            Log.w(TAG, "a message the page sent cannot be read: ${e.message}")
            return
        }
        when (message.optString("type")) {
            "hello" -> {
                page = reply
                reply.postMessage(appMessage())
                player?.report(withQueue = true)
            }
            "player" -> player?.handle(message) ?: waiting.add(message)
            "device" -> reply.postMessage(appMessage())
            "device-set" -> {
                message.optString("player").trim().takeIf { it.isNotEmpty() }?.let { prefs.playerName = it }
                if (message.has("stayReady")) {
                    prefs.stayReady = message.optBoolean("stayReady")
                    PlayerService.ready(this)
                }
                reply.postMessage(appMessage())
            }
            "battery" -> askToRunInBackground()
            "address" -> askForAddress()
            "open" -> message.optString("url").takeIf { it.isNotEmpty() }?.let { openOutside(it) }
        }
    }

    /** This device's settings, for the page's "This device" section. */
    private fun appMessage(): String = JSONObject()
        .put("type", "app")
        .put("version", BuildConfig.VERSION_NAME)
        .put("player", prefs.playerName)
        .put("stayReady", prefs.stayReady)
        .put("server", prefs.serverUrl ?: "")
        .put("batteryAllowed", getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(packageName))
        .toString()

    /** Android's own dialog: let the app run in the background (left out of battery optimisation). */
    private fun askToRunInBackground() {
        try {
            startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName")))
        } catch (e: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
        }
    }

    private fun openOutside(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (e: ActivityNotFoundException) {
            Log.w(TAG, "nothing opens $url")
        }
    }

    private fun askForAddress() {
        if (dialog?.isShowing == true) return
        val field = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            hint = getString(R.string.server_address_hint)
            setText(prefs.serverUrl ?: "")
            setSingleLine()
        }
        val box = FrameLayout(this).apply {
            val side = (24 * resources.displayMetrics.density).toInt()
            setPadding(side, side / 2, side, 0)
            addView(field)
        }
        dialog = AlertDialog.Builder(this)
            .setTitle(R.string.server_address)
            .setView(box)
            .setCancelable(false)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val typed = field.text.toString()
                if (typed.isBlank()) {
                    askForAddress()
                } else {
                    val url = Prefs.normalise(typed)
                    prefs.serverUrl = url
                    show(url)
                }
            }
            .show()
    }

    private fun cannotReach(url: String, why: String) {
        if (dialog?.isShowing == true || isFinishing) return
        dialog = AlertDialog.Builder(this)
            .setTitle(getString(R.string.cannot_reach, url))
            .setMessage(why.takeIf { it.isNotBlank() })
            .setCancelable(false)
            .setPositiveButton(R.string.try_again) { _, _ -> show(url) }
            .setNegativeButton(R.string.change_address) { _, _ -> askForAddress() }
            .show()
    }

    /** Cratarr's Settings page from the launcher (a long press on the icon): "This device" is at its top. */
    private fun addSettingsShortcut() {
        val shortcut = ShortcutInfo.Builder(this, "settings")
            .setShortLabel(getString(R.string.settings))
            .setIcon(Icon.createWithResource(this, R.mipmap.ic_launcher))
            .setIntent(Intent(this, MainActivity::class.java).setAction(Intent.ACTION_VIEW).putExtra(EXTRA_SETTINGS, true))
            .build()
        getSystemService(ShortcutManager::class.java)?.dynamicShortcuts = listOf(shortcut)
    }

    private fun askForNotifications() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }

    private companion object {
        const val TAG = "Cratarr"
        const val EXTRA_SETTINGS = "cratarr.SETTINGS"
    }
}
