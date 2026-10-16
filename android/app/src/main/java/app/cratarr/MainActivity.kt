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
import android.content.SharedPreferences
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
import android.webkit.HttpAuthHandler
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
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

@androidx.annotation.OptIn(UnstableApi::class)
class MainActivity : AppCompatActivity() {
    private lateinit var prefs: Prefs
    private lateinit var frame: FrameLayout
    private var webView: WebView? = null

    /** The address the WebView shows. */
    private var shown: String? = null

    /** The page's settings, put into its storage before it starts (so both addresses' pages share them). */
    private var pageScript: ScriptHandler? = null

    /** The other address taken into use ([Reach]): the page from there. */
    private val inUseChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key != Prefs.IN_USE) return@OnSharedPreferenceChangeListener
        runOnUiThread {
            val url = prefs.serverUrl
            if (url != null && shown != null && url != shown && lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) show(url)
        }
    }

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

    /** The downloads' progress goes to the page as it changes, a second apart at most. */
    private val downloadsChanged = object : DownloadManager.Listener {
        override fun onDownloadChanged(manager: DownloadManager, download: Download, finalException: Exception?) {
            tellDownloads()
            tellKept()
        }
        override fun onDownloadRemoved(manager: DownloadManager, download: Download) = tellDownloads()
    }
    private var downloadsTold = 0L
    private var downloadsPending = false
    private var keptTold = 0L
    private var keptPending = false

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
                val connected = PagePlayer(this, controllerFuture.get()) { message -> page?.postMessage(message) }
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
        Downloads.manager(this).addListener(downloadsChanged)
        // Kept on this device (decision 0038): in step now, and every few hours from here on.
        Kept.onChange = { runOnUiThread { tellKept() } }
        Kept.schedule(this)
        Kept.sync(this)
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(inUseChanged)
        Reach.watch(this)
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
        when {
            prefs.serverUrl == null -> askForAddress()
            // The first page: from the address that answers.
            shown == null -> lifecycleScope.launch {
                withContext(Dispatchers.IO) { Reach.check(this@MainActivity) }
                prefs.serverUrl?.let { if (it != shown) show(it) }
            }
            else -> {
                prefs.serverUrl?.let { if (it != shown) show(it) }
                Reach.checkSoon(this)
            }
        }
        PlayerService.ready(this)
    }

    override fun onDestroy() {
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(inUseChanged)
        Downloads.manager(this).removeListener(downloadsChanged)
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
        pageScript = null
        // Either address's pages stay in the app and may talk to it.
        val origins = prefs.origins() + listOfNotNull(Prefs.originOf(url))
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
        signInTried = null
        view.webViewClient = CratarrWebViewClient(origins, onError = { cannotReach(url, it) }, onGone = { show(url) }, onSignIn = ::signIn)
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
        // `window.CratarrApp`, on Cratarr's pages only.
        if (origins.isNotEmpty() && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(view, "CratarrApp", origins) { _, message, _, isMainFrame, reply ->
                val data = message.data
                if (isMainFrame && data != null) onPageMessage(data, reply)
            }
        }
        frame.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        webView = view
        keepPagePrefs(view)
        shown = url
        view.loadUrl(if (openSettings) "${url.trimEnd('/')}/#/settings" else url)
        openSettings = false
    }

    /** A message from the page: `hello`, `player` (its player's calls), `device` and `device-set` (this device's
     *  settings), `page-prefs` (its own settings), `network-test`, `battery`, `address`, `open` (a link outside). */
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
            // Settings' "Test both connections".
            "network-test" -> lifecycleScope.launch {
                val (hasLocal, local, public) = withContext(Dispatchers.IO) { Reach.test(this@MainActivity) }
                reply.postMessage(
                    JSONObject()
                        .put("type", "network-test")
                        .put("local", if (hasLocal) local == 200 else JSONObject.NULL)
                        .put("public", if (prefs.publicAddress != null) public == 200 else JSONObject.NULL)
                        .put("publicRefused", public == 401 || public == 403)
                        .toString(),
                )
                Reach.checkSoon(this@MainActivity)
            }
            "page-prefs" -> message.optString("prefs").takeIf { it.isNotEmpty() && it != prefs.pagePrefs }?.let {
                prefs.pagePrefs = it
                webView?.let(::keepPagePrefs)
            }
            "device-set" -> {
                message.optString("player").trim().takeIf { it.isNotEmpty() }?.let { prefs.playerName = it }
                if (message.has("stayReady")) {
                    prefs.stayReady = message.optBoolean("stayReady")
                    PlayerService.ready(this)
                }
                message.optString("downloadQuality").takeIf { it.isNotEmpty() }?.let { prefs.downloadQuality = it }
                if (message.has("levelling")) prefs.levelling = message.optBoolean("levelling")
                if (message.has("downloadOverMobile") || message.has("syncOnlyCharging")) {
                    if (message.has("downloadOverMobile")) prefs.downloadOverMobile = message.optBoolean("downloadOverMobile")
                    if (message.has("syncOnlyCharging")) prefs.syncOnlyCharging = message.optBoolean("syncOnlyCharging")
                    Downloads.manager(this).requirements = Downloads.requirements(prefs)
                    Kept.schedule(this)
                }
                if (message.has("syncBudgetGb")) {
                    prefs.syncBudgetGb = message.optLong("syncBudgetGb", 20L)
                    Kept.sync(this)
                }
                // The sign-in: a user name (empty: none, and no password), and a password only when one is typed.
                if (message.has("signInUser")) {
                    val user = message.optString("signInUser").trim()
                    prefs.signInUser = user.takeIf { it.isNotEmpty() }
                    if (user.isEmpty()) prefs.signInPassword = null
                }
                message.optString("signInPassword").takeIf { it.isNotEmpty() && message.has("signInPassword") }?.let { prefs.signInPassword = it }
                // The addresses as typed (empty local: none), and whether the local one is preferred. Each is kept where
                // Cratarr answers ([Reach.find]: `https://` first when no scheme is typed), so a stray edit does not take
                // the app off Cratarr; a local network address where nothing answers too (away from home, it cannot).
                // One not kept is named back to the page, whose field shows the address kept again.
                fun same(typed: String, kept: String?) = typed == kept || Prefs.normalise(typed) == kept
                val local = if (message.has("local")) message.optString("local").trim().takeUnless { same(it, prefs.localAddress) } else null
                val preferLocal = if (message.has("preferLocal")) message.optBoolean("preferLocal") else prefs.preferLocal
                // `server`: from pages before there were two.
                val public = (if (message.has("public")) message.optString("public") else message.optString("server"))
                    .trim().takeIf { it.isNotEmpty() && !same(it, prefs.publicAddress) }
                if ((local != null && (local.isNotEmpty() || prefs.localAddress != null)) || preferLocal != prefs.preferLocal || public != null) {
                    lifecycleScope.launch {
                        var refused: String? = null
                        if (local != null) {
                            if (local.isEmpty()) {
                                prefs.localAddress = null
                            } else {
                                val (found, answered) = withContext(Dispatchers.IO) { Reach.find(this@MainActivity, local) }
                                when {
                                    found != null -> prefs.localAddress = found
                                    !answered -> prefs.localAddress = Prefs.normalise(local)
                                    else -> refused = local
                                }
                            }
                        }
                        prefs.preferLocal = preferLocal
                        if (public != null) {
                            val (found, _) = withContext(Dispatchers.IO) { Reach.find(this@MainActivity, public) }
                            if (found != null) prefs.publicAddress = found else refused = public
                        }
                        // Which of them answers now: the page comes from there.
                        withContext(Dispatchers.IO) { Reach.check(this@MainActivity) }
                        reply.postMessage(appMessage(refused))
                    }
                    return
                }
                reply.postMessage(appMessage())
            }
            // The owner's downloads (decision 0033, step 4): tracks as the page describes them.
            "download" -> {
                Downloads.add(this, message.optJSONArray("tracks") ?: JSONArray())
                tellDownloads()
            }
            "downloads" -> tellDownloads(now = true)
            // Kept on this device (decision 0038).
            "kept" -> tellKept(now = true)
            "kept-set" -> {
                val kind = message.optString("kind")
                val source = message.optString("source")
                val id = message.optString("id")
                if (kind in setOf("album", "playlist", "crate", "artist") && source.isNotEmpty() && id.isNotEmpty()) {
                    Kept.set(this, kind, source, id, message.optString("name"), message.optBoolean("on"))
                }
            }
            "kept-sync" -> Kept.sync(this, retryFailed = true)
            "download-remove" -> {
                if (message.optBoolean("all")) {
                    Downloads.removeAll(this)
                } else {
                    val ids = message.optJSONArray("ids") ?: JSONArray()
                    Downloads.remove(this, (0 until ids.length()).map(ids::optString))
                }
                tellDownloads()
            }
            "battery" -> askToRunInBackground()
            "address" -> askForAddress()
            "open" -> message.optString("url").takeIf { it.isNotEmpty() }?.let { openOutside(it) }
            // Share (a menu's): Android's share sheet, as the WebView has no navigator.share. `text` holds the link.
            "share" -> {
                val send = Intent(Intent.ACTION_SEND)
                    .setType("text/plain")
                    .putExtra(Intent.EXTRA_SUBJECT, message.optString("title"))
                    .putExtra(Intent.EXTRA_TEXT, message.optString("text"))
                try {
                    startActivity(Intent.createChooser(send, null))
                } catch (e: ActivityNotFoundException) {
                    Log.w(TAG, "nothing shares text")
                }
            }
        }
    }

    /** This device's settings, for the page's "This device" section. */
    /** The app's settings for the page; with [refused], an address typed there where Cratarr does not answer. */
    private fun appMessage(refused: String? = null): String = JSONObject()
        .put("type", "app")
        .put("refused", refused ?: "")
        .put("version", BuildConfig.VERSION_NAME)
        .put("player", prefs.playerName)
        .put("stayReady", prefs.stayReady)
        // `server`: the public address, for pages from before there were two.
        .put("server", prefs.publicAddress ?: "")
        .put("public", prefs.publicAddress ?: "")
        .put("local", prefs.localAddress ?: "")
        .put("preferLocal", prefs.preferLocal)
        // The sign-in's user name, and whether a password is kept (it is never shown).
        .put("signInUser", prefs.signInUser ?: "")
        .put("signInSet", prefs.signInPassword != null)
        .put("inUse", prefs.serverUrl ?: "")
        .put("batteryAllowed", getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(packageName))
        .put("downloadQuality", prefs.downloadQuality)
        .put("downloadOverMobile", prefs.downloadOverMobile)
        .put("syncBudgetGb", prefs.syncBudgetGb)
        .put("syncOnlyCharging", prefs.syncOnlyCharging)
        .put("levelling", prefs.levelling)
        .toString()

    /** The kept items' progress (decision 0038), read off the main thread, to the page; changes close together go as one. */
    private fun tellKept(now: Boolean = false) {
        if (page == null || keptPending) return
        val wait = if (now) 0L else (1_000L - (System.currentTimeMillis() - keptTold)).coerceAtLeast(0L)
        keptPending = true
        lifecycleScope.launch {
            delay(wait)
            val report = withContext(Dispatchers.IO) { Kept.report(this@MainActivity).toString() }
            keptPending = false
            keptTold = System.currentTimeMillis()
            page?.postMessage(report)
        }
    }

    /** The downloads, read off the main thread, to the page; changes close together go as one. */
    private fun tellDownloads(now: Boolean = false) {
        if (page == null || downloadsPending) return
        val wait = if (now) 0L else (1_000L - (System.currentTimeMillis() - downloadsTold)).coerceAtLeast(0L)
        downloadsPending = true
        lifecycleScope.launch {
            delay(wait)
            val list = withContext(Dispatchers.IO) { Downloads.list(this@MainActivity).toString() }
            downloadsPending = false
            downloadsTold = System.currentTimeMillis()
            page?.postMessage(list)
        }
    }

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

    /** The sign-in given to this page (the proxy asks again when it was wrong). */
    private var signInTried: String? = null

    /** Away from home the proxy asks for a Cratarr account: the saved one, once; asked for when there is none, or when
     *  it was refused. What is typed is kept (Settings → Network) for the app's own requests too. */
    private fun signIn(handler: HttpAuthHandler) {
        val user = prefs.signInUser
        val password = prefs.signInPassword
        val tried = "$user\n$password"
        if (user != null && password != null && signInTried != tried) {
            signInTried = tried
            handler.proceed(user, password)
            return
        }
        askSignIn(handler, refused = user != null && signInTried == tried)
    }

    private fun askSignIn(handler: HttpAuthHandler, refused: Boolean) {
        if (isFinishing) return handler.cancel()
        dialog?.dismiss()
        val side = (24 * resources.displayMetrics.density).toInt()
        val name = EditText(this).apply {
            hint = getString(R.string.user_name)
            setText(prefs.signInUser ?: "")
            setSingleLine()
        }
        val password = EditText(this).apply {
            hint = getString(R.string.password)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            setSingleLine()
        }
        val box = android.widget.LinearLayout(this).apply {
            orientation = android.widget.LinearLayout.VERTICAL
            setPadding(side, side / 2, side, 0)
            addView(name)
            addView(password)
        }
        dialog = AlertDialog.Builder(this)
            .setTitle(R.string.sign_in)
            .setMessage(if (refused) getString(R.string.sign_in_refused) else null)
            .setView(box)
            .setCancelable(false)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val u = name.text.toString().trim()
                val p = password.text.toString()
                prefs.signInUser = u
                prefs.signInPassword = p
                signInTried = "$u\n$p"
                handler.proceed(u, p)
            }
            .setNegativeButton(android.R.string.cancel) { _, _ -> handler.cancel() }
            .show()
    }

    /** The page's settings as the app keeps them, into the page's storage before its script runs, on either address. */
    private fun keepPagePrefs(view: WebView) {
        pageScript?.remove()
        pageScript = null
        val saved = prefs.pagePrefs ?: return
        val origins = prefs.origins()
        if (origins.isEmpty() || !WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) return
        pageScript = WebViewCompat.addDocumentStartJavaScript(view, "try { localStorage.setItem('cratarr-prefs', ${JSONObject.quote(saved)}) } catch (e) {}", origins)
    }

    /** Cratarr's address, asked for: kept where Cratarr answers ([Reach.find]); else asked again, [refused] named. */
    private fun askForAddress(refused: String? = null) {
        if (dialog?.isShowing == true) return
        val field = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            hint = getString(R.string.server_address_hint)
            setText(refused ?: prefs.publicAddress ?: "")
            setSingleLine()
        }
        val box = FrameLayout(this).apply {
            val side = (24 * resources.displayMetrics.density).toInt()
            setPadding(side, side / 2, side, 0)
            addView(field)
        }
        dialog = AlertDialog.Builder(this)
            .setTitle(R.string.server_address)
            .setMessage(refused?.let { getString(R.string.not_cratarr, it) })
            .setView(box)
            .setCancelable(false)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val typed = field.text.toString().trim()
                if (typed.isEmpty()) {
                    askForAddress()
                } else {
                    lifecycleScope.launch {
                        val (found, _) = withContext(Dispatchers.IO) { Reach.find(this@MainActivity, typed) }
                        if (found == null) {
                            askForAddress(typed)
                            return@launch
                        }
                        prefs.publicAddress = found
                        prefs.use(found)
                        withContext(Dispatchers.IO) { Reach.check(this@MainActivity) }
                        prefs.serverUrl?.let(::show)
                    }
                }
            }
            .show()
    }

    private fun cannotReach(url: String, why: String) {
        if (dialog?.isShowing == true || isFinishing) return
        lifecycleScope.launch {
            // The other address may answer (off the local network, say): its page then, without a word.
            val now = withContext(Dispatchers.IO) { Reach.check(this@MainActivity) }
            if (now != null && now != url) {
                if (now != shown) show(now)
                return@launch
            }
            if (dialog?.isShowing == true || isFinishing) return@launch
            dialog = AlertDialog.Builder(this@MainActivity)
                .setTitle(getString(R.string.cannot_reach, url))
                .setMessage(why.takeIf { it.isNotBlank() })
                .setCancelable(false)
                .setPositiveButton(R.string.try_again) { _, _ -> show(url) }
                .setNegativeButton(R.string.change_address) { _, _ -> askForAddress() }
                .show()
        }
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
