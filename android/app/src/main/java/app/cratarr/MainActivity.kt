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
import android.database.ContentObserver
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.MediaStore
import android.provider.Settings
import android.text.InputType
import android.util.Log
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.webkit.CookieManager
import android.webkit.ValueCallback
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
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
import androidx.webkit.ServiceWorkerClientCompat
import androidx.webkit.ServiceWorkerControllerCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
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
    /** The keyboard's height over the page, as last told to it (page pixels; 0 while it is down). */
    private var keyboardOver = 0

    /** The app's player (PlayerService), once connected; the page's player calls before that wait in `waiting`. */
    private lateinit var controllerFuture: ListenableFuture<MediaController>
    private var player: PagePlayer? = null
    private val waiting = mutableListOf<JSONObject>()
    private var dialog: AlertDialog? = null

    /** Local mode (decision 0052): Android's permission for the device's music, asked for as it is first opened. */
    private val deviceMusicAsk = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        prefs.deviceMusicAsked = true
        // The folder whose switch asked for it goes on with it.
        folderAsked?.takeIf { granted }?.let { prefs.localFolders = prefs.localFolders + it }
        folderAsked = null
        page?.postMessage(appMessage())
        page?.let(::tellFolders)
    }

    /** The folder turned on before Android's permission was there: on once it is given. */
    private var folderAsked: String? = null

    /** Android's index of the device's music changed: the page is told once things settle (a copy is many changes). */
    private val deviceMusicChanged = object : ContentObserver(Handler(Looper.getMainLooper())) {
        private val tell = Runnable { page?.postMessage(JSONObject().put("type", "device-library-changed").toString()) }

        override fun onChange(selfChange: Boolean) {
            if (!DeviceMusic.granted(this@MainActivity)) return
            frame.removeCallbacks(tell)
            frame.postDelayed(tell, 2_000)
        }
    }

    /** The page's visuals want the sound as it is heard (LiveSound): while the page is in view. */
    private var liveWanted = false

    private fun live() {
        LiveSound.listener = if (liveWanted && lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) { m -> page?.postMessage(m) } else null
    }

    override fun onStart() {
        super.onStart()
        Notices.inFront = true
        live()
    }

    override fun onStop() {
        Notices.inFront = false
        LiveSound.listener = null
        super.onStop()
    }

    /** The mode changed (here, or by a play from Cratarr): the page hears it. Where the controls point changed (the
     *  service, decision 0062): the page hears it and why. */
    private val modeChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == Prefs.MODE) runOnUiThread { page?.postMessage(appMessage()) }
        if (key == Prefs.TARGET) runOnUiThread {
            page?.postMessage(JSONObject().put("type", "target").put("target", prefs.target).put("why", prefs.targetWhy).toString())
        }
    }

    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private val fileChooser = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        fileCallback?.onReceiveValue(FileChooserParams.parseResult(result.resultCode, result.data))
        fileCallback = null
    }

    /** Download… (0.10): the page's export while the user says where it is saved, and the page that asked. */
    private var saveAsked: Pair<JSONObject, JavaScriptReplyProxy>? = null
    private val exportSave = registerForActivityResult(ExportSave.SaveAs()) { doc ->
        val (message, reply) = saveAsked ?: return@registerForActivityResult
        saveAsked = null
        val token = message.optString("token")
        if (doc == null) {
            reply.postMessage(ExportSave.message(token, "cancelled").toString())
            return@registerForActivityResult
        }
        ExportSave.start(this, ExportSave.Job(doc, token, message.optString("name")))
    }

    /** Where the first page opens, past the server's address: Settings (the launcher's Settings shortcut), or a
     *  connection's own page (back from a sign-in in the browser). */
    private var openAt: String? = null

    /** Where an intent asks the page to open: `#/settings` for the Settings shortcut; for `cratarr://connected/<service>`
     *  (the way back from a sign-in, with what went wrong as `scrobble`), that connection's page in Settings; Spotify's
     *  (0.9.9, decision 0073) is the account's own, what went wrong as `spotify_error`. */
    private fun openAtOf(intent: Intent?): String? {
        if (intent == null) return null
        if (intent.getBooleanExtra(EXTRA_SETTINGS, false)) return "#/settings"
        val data = intent.data ?: return null
        if (data.scheme != "cratarr" || data.host != "connected") return null
        val service = data.pathSegments.firstOrNull()?.takeIf { it.matches(Regex("[a-z]+")) } ?: return null
        if (service == "spotify") {
            val why = data.getQueryParameter("spotify_error")?.let { "&spotify_error=" + Uri.encode(it) } ?: ""
            return "#/settings?view=settings%3Aaccount%2Fspotify$why"
        }
        val why = data.getQueryParameter("scrobble")?.let { "&scrobble=" + Uri.encode(it) } ?: ""
        return "#/settings?view=settings%3Aconnections%2F$service$why"
    }

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
        LinkLog.catchCrashes(this)
        prefs = Prefs(this)
        openAt = openAtOf(intent)
        frame = FrameLayout(this)
        setContentView(frame)
        ViewCompat.setOnApplyWindowInsetsListener(frame) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            // The keyboard's height over the page (the owner, 2026-10-25: the bottom glass's search field went behind
            // it): Android's own measure, above the system bar the page already stands on, in the page's pixels. The
            // page puts what floats along the bottom just above it.
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
            val over = ((ime - bars.bottom).coerceAtLeast(0) / resources.displayMetrics.density).toInt()
            if (over != keyboardOver) {
                keyboardOver = over
                page?.postMessage(JSONObject().put("type", "keyboard").put("height", over).toString())
            }
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
        // A download being saved reports to the page in view, whichever page asked for it.
        ExportSave.onReport = { m -> runOnUiThread { page?.postMessage(m) } }
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(inUseChanged)
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(modeChanged)
        Reach.watch(this)
        // An output coming or going: the page's device sheet shows the outputs there are now.
        outputs = Output.Watch(this) { page?.postMessage(appMessage()) }
        // The network as it changes (0.9.15): the page knows it is out of reach without waiting for an answer.
        network = NetState.Watch(this) { page?.postMessage(it.toString()) }
        Plays.onPlayedHere = { runOnUiThread { page?.postMessage(Plays.playedHere(this).toString()) } }
        // The device's music changed (decision 0052): the page asks for it again when it shows it.
        contentResolver.registerContentObserver(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, true, deviceMusicChanged)
    }

    /** Watches the outputs while the page is shown (decision 0033, amended 2026-10-19). */
    private var outputs: Output.Watch? = null
    private var network: NetState.Watch? = null

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val at = openAtOf(intent) ?: return
        val base = shown
        when {
            page == null || base == null -> openAt = at
            at == "#/settings" -> webView?.evaluateJavascript("location.hash = '#/settings'", null)
            // Back from a sign-in: the page anew, so it reads the connection as it is now.
            else -> webView?.loadUrl("${base.trimEnd('/')}/$at")
        }
    }

    override fun onResume() {
        super.onResume()
        // Back from Android's background dialog, say: the page shows the settings as they are now.
        page?.postMessage(appMessage())
        when {
            prefs.serverUrl == null -> askForAddress()
            // The first page: from the address that answers, looked for a moment at most (0.9.15: no wait out of
            // reach); without a network, at once. An answer later from the other address moves the page there.
            shown == null -> lifecycleScope.launch {
                if (NetState.connected(this@MainActivity)) {
                    val looking = lifecycleScope.async(Dispatchers.IO) { Reach.check(this@MainActivity) }
                    withTimeoutOrNull(FIRST_LOOK_MS) { looking.await() }
                } else {
                    Reach.checkSoon(this@MainActivity)
                }
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
        LiveSound.listener = null
        outputs?.stop()
        network?.stop()
        Plays.onPlayedHere = null
        ExportSave.onReport = null
        contentResolver.unregisterContentObserver(deviceMusicChanged)
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(inUseChanged)
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(modeChanged)
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
            // Its sign-ins are named for the app and its player in Settings → Authentication (decision 0043), not as
            // the Chrome its WebView says it is.
            userAgentString = "$userAgentString CratarrApp/${BuildConfig.VERSION_NAME} (name=${Uri.encode(prefs.playerName)})"
        }
        // The page keeps its priority when not in view: it is the player.
        view.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
        val app = applicationContext
        view.webViewClient = CratarrWebViewClient(
            origins,
            onError = { cannotReach(url, it) },
            onGone = { show(url) },
            deviceCover = { DeviceMusic.cover(app, it) },
            // One sign-in for both addresses (0.12, the owner 2026-10-30): the session a page loaded with goes to the
            // other address too; a sign-in page with the other's session at hand takes it and loads again.
            onLoaded = { v, loaded -> if (Sessions.loaded(app, loaded) && v === webView) v.reload() },
        )
        // The page's service worker asks for the page's files too (decision 0085): from the app's copy, as above.
        if (WebViewFeature.isFeatureSupported(WebViewFeature.SERVICE_WORKER_BASIC_USAGE) &&
            WebViewFeature.isFeatureSupported(WebViewFeature.SERVICE_WORKER_SHOULD_INTERCEPT_REQUEST)
        ) {
            ServiceWorkerControllerCompat.getInstance().setServiceWorkerClient(object : ServiceWorkerClientCompat() {
                override fun shouldInterceptRequest(request: WebResourceRequest): WebResourceResponse? =
                    if (Prefs.originOf(request.url.toString()) in origins) BundledPage.answer(app, request.method, request.url) else null
            })
        }
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
        // `window.CratarrApp`, on Cratarr's pages only: the local address's over plain http only while it answers as
        // this app's Cratarr on this network (Reach.trusts).
        if (origins.isNotEmpty() && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(view, "CratarrApp", origins) { _, message, sourceOrigin, isMainFrame, reply ->
                val data = message.data
                if (isMainFrame && data != null && Reach.trusts(this, sourceOrigin.toString())) onPageMessage(data, reply)
            }
        }
        frame.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        webView = view
        keepPagePrefs(view)
        shown = url
        // The other address's session, when this one has none: no sign-in page for a moment as the address changes.
        Sessions.toward(app, url)
        view.loadUrl(openAt?.let { "${url.trimEnd('/')}/$it" } ?: url)
        openAt = null
    }

    /** A message from the page: `hello`, `player` (its player's calls), `target` (where the controls point), `device`
     *  and `device-set` (this device's settings), `page-prefs` (its own settings), `network-test`, `battery`,
     *  `address`, `open` (a link outside), `export-save` (Download…: an export saved as its one file), `snippet` (a
     *  track's snippet, 0.12). */
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
                // A new page asks for the sound itself when its visuals open.
                liveWanted = false
                live()
                reply.postMessage(appMessage())
                // The network as it is, and the device music played here (0.9.15, offline).
                reply.postMessage(NetState.now(this).toString())
                reply.postMessage(Plays.playedHere(this).toString())
                player?.report(withQueue = true)
                // A download being saved: where it is.
                ExportSave.latest?.let { reply.postMessage(it) }
                followSession()
            }
            "network" -> reply.postMessage(NetState.now(this).toString())
            "device-plays" -> reply.postMessage(Plays.playedHere(this).toString())
            "player" -> player?.handle(message) ?: waiting.add(message)
            // A track's snippet (0.12): played beside the music by the player service. Before the player is connected
            // it is let go: a snippet asked then would sound late, after its menu.
            "snippet" -> player?.snippet(message)
            // Where this device's controls point (decision 0062): asked of the player service, which decides and
            // tells the page (modeChanged). Started for it while the app is in front.
            "target" -> {
                PlayerService.ready(this)
                player?.pointAt(message.optString("to")) ?: waiting.add(message)
            }
            "device" -> reply.postMessage(appMessage())
            // The visuals' sound as it is heard (decision 0046, amended 2026-10-19), on or off.
            "live" -> {
                liveWanted = message.optBoolean("on")
                live()
            }
            // Local mode (decision 0052): the switch (Android's permission asked for first), and the device's music.
            "device-music" -> when (message.optString("op")) {
                // Into local mode only with a folder turned on (the switch shows only then).
                "mode" -> {
                    val to = message.optString("mode")
                    if (to == Prefs.MODE_DEVICE && !DeviceMusic.on(this)) reply.postMessage(appMessage()) else switchMode(to)
                }
                // Settings: Android's permission, the folders it found music in, and one turned on or off.
                // Asked once and refused for good, Android shows no dialog again: its app settings, where it is given.
                "allow" -> when {
                    DeviceMusic.granted(this) -> reply.postMessage(appMessage())
                    prefs.deviceMusicAsked && !shouldShowRequestPermissionRationale(DeviceMusic.permission) -> openAppSettings()
                    else -> deviceMusicAsk.launch(DeviceMusic.permission)
                }
                // The switch turned off (the owner, 2026-10-25): a permission is taken back in Android's app settings only.
                "settings" -> openAppSettings()
                "folders" -> tellFolders(reply)
                "folder" -> {
                    val path = message.optString("path")
                    val on = message.optBoolean("on")
                    when {
                        path.isEmpty() -> Unit
                        on && !DeviceMusic.granted(this) -> {
                            folderAsked = path
                            deviceMusicAsk.launch(DeviceMusic.permission)
                        }
                        else -> {
                            prefs.localFolders = if (on) prefs.localFolders + path else prefs.localFolders - path
                            // The last one off: local mode is gone, and Cratarr's music is back.
                            if (!DeviceMusic.on(this) && prefs.mode == Prefs.MODE_DEVICE) switchMode(Prefs.MODE_CRATARR)
                            reply.postMessage(appMessage())
                            tellFolders(reply)
                        }
                    }
                }
                "library" -> lifecycleScope.launch {
                    val out = JSONObject().put("type", "device-library")
                    if (DeviceMusic.granted(this@MainActivity)) {
                        out.put("tracks", withContext(Dispatchers.IO) { DeviceMusic.tracks(this@MainActivity) })
                    } else {
                        out.put("denied", true)
                    }
                    reply.postMessage(out.toString())
                }
            }
            // Settings' "Test both connections".
            "network-test" -> lifecycleScope.launch {
                val (hasLocal, local, public) = withContext(Dispatchers.IO) { Reach.test(this@MainActivity) }
                reply.postMessage(
                    JSONObject()
                        .put("type", "network-test")
                        .put("local", if (hasLocal) local == 200 else JSONObject.NULL)
                        .put("public", if (prefs.publicAddress != null) public == 200 else JSONObject.NULL)
                        // Cratarr asks this device to sign in (decision 0039): no session yet, or it ended.
                        .put("signedOut", public == 401)
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
                // This device's output (decision 0033, amended 2026-10-19): an output's id, or null for Automatic.
                if (message.has("output")) {
                    prefs.outputChoice = if (message.isNull("output")) "" else message.optString("output")
                    prefs.outputChoiceSet = Output.set(this)
                }
                // Automatic by the owner's order: on or off, the order after a drag, an output not there forgotten.
                if (message.has("ordered")) prefs.outputOrdered = message.optBoolean("ordered")
                message.optJSONArray("order")?.let { a -> Output.order(this, (0 until a.length()).map(a::optString)) }
                message.optString("forget").takeIf { it.isNotEmpty() }?.let { Output.forget(this, it) }
                if (message.has("stayReady")) {
                    prefs.stayReady = message.optBoolean("stayReady")
                    PlayerService.ready(this)
                }
                message.optString("downloadQuality").takeIf { it.isNotEmpty() }?.let { prefs.downloadQuality = it }
                if (message.has("levelling")) prefs.levelling = message.optBoolean("levelling")
                // Spike B (0.11): the players' rules from Rust, off by default.
                if (message.has("rustRules")) {
                    prefs.rustRules = message.optBoolean("rustRules")
                    RustRules.set(this, prefs.rustRules)
                }
                message.optString("levelUnknown").takeIf { it.isNotEmpty() }?.let { prefs.levelUnknown = it }
                // Start and stop (0.9.6): the page's This device setting, kept here for the app's own player.
                message.optJSONObject("startStop")?.let { s ->
                    prefs.startStopMode = s.optString("mode", Prefs.START_STOP_OFF)
                    prefs.startStopMs = s.optLong("ms", 800L)
                    if (s.has("stopMs")) prefs.startStopStopMs = s.optLong("stopMs", 800L)
                    prefs.startStopSkips = s.optBoolean("skips")
                    prefs.startStopRewind = s.optBoolean("rewind")
                    // The Tape page (decision 0074): the tape's mechanics and its four motions, each kept as it is when the
                    // page leaves it out.
                    prefs.setTape(s)
                    if (s.has("edges")) prefs.windEdges = s.optString("edges", "stop")
                }
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
            // Download… (0.10): Cratarr prepared the export and named its download (a zip, or the one file); the user
            // says where it is saved, in Android's save dialog with that name.
            "export-save" -> {
                val token = message.optString("token")
                val name = message.optString("name")
                if (token.isEmpty() || name.isEmpty()) {
                    reply.postMessage(ExportSave.message(token, "failed").put("error", "Nothing to save.").toString())
                    return
                }
                saveAsked = message to reply
                try {
                    exportSave.launch(name)
                } catch (e: ActivityNotFoundException) {
                    saveAsked = null
                    reply.postMessage(ExportSave.message(token, "failed").put("error", "No app on this device saves a file.").toString())
                }
            }
            "download-remove" -> {
                if (message.optBoolean("all")) {
                    Downloads.removeAll(this)
                } else {
                    val ids = message.optJSONArray("ids") ?: JSONArray()
                    Downloads.remove(this, (0 until ids.length()).map(ids::optString))
                }
                tellDownloads()
            }
            // The Background switch (the owner, 2026-10-25): on asks Android's dialog; off opens this app's page, its
            // "App battery usage" one tap away (Android's list of every app made the user look for Cratarr in it).
            "battery" -> if (message.optString("op") == "settings") openAppSettings() else askToRunInBackground()
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

    /** The folders Android found music in, read off the main thread, to the page (or that it may not read them). */
    private fun tellFolders(reply: JavaScriptReplyProxy) {
        lifecycleScope.launch {
            val out = JSONObject().put("type", "device-folders")
            if (DeviceMusic.granted(this@MainActivity)) {
                out.put("folders", withContext(Dispatchers.IO) { DeviceMusic.folders(this@MainActivity) })
            } else {
                out.put("denied", true)
            }
            reply.postMessage(out.toString())
        }
    }

    /** Local mode on or off (decision 0052), by the player, once it is connected. */
    private fun switchMode(to: String) {
        val m = JSONObject().put("type", "player").put("op", "mode").put("mode", to)
        player?.handle(m) ?: waiting.add(m)
    }

    /** The app's settings for the page; with [refused], an address typed there where Cratarr does not answer. */
    private fun appMessage(refused: String? = null): String = JSONObject()
        .put("type", "app")
        .put("refused", refused ?: "")
        .put("version", BuildConfig.VERSION_NAME)
        // A press the other way during a Start and stop ramp turns it round from where it is (0075); the page's play
        // button is live during one only for an app that does.
        .put("turnsRound", true)
        .put("player", prefs.playerName)
        // Its outputs, and the one chosen (null: Automatic); `output` in Play On's reports is the one that plays.
        .put("outputs", org.json.JSONArray(Output.all(this)))
        .put("output", prefs.outputChoice.ifEmpty { null } ?: org.json.JSONObject.NULL)
        // Automatic by the owner's order, and every output seen here in that order (`outputs`: those there now).
        .put("ordered", prefs.outputOrdered)
        .put("known", org.json.JSONArray(Output.known(this)))
        .put("stayReady", prefs.stayReady)
        // `server`: the public address, for pages from before there were two.
        .put("server", prefs.publicAddress ?: "")
        .put("public", prefs.publicAddress ?: "")
        .put("local", prefs.localAddress ?: "")
        // The local address refused the sign-in a while ago (0.9.7: its server does not know this session), so the
        // app goes by the public one meanwhile: the page says so.
        .put("localRefused", prefs.localAddress?.let { Net.refused(it) } ?: false)
        .put("preferLocal", prefs.preferLocal)
        .put("inUse", prefs.serverUrl ?: "")
        .put("batteryAllowed", getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(packageName))
        .put("downloadQuality", prefs.downloadQuality)
        .put("downloadOverMobile", prefs.downloadOverMobile)
        .put("syncBudgetGb", prefs.syncBudgetGb)
        .put("syncOnlyCharging", prefs.syncOnlyCharging)
        .put("levelling", prefs.levelling)
        .put("rustRules", prefs.rustRules)
        .put("levelUnknown", prefs.levelUnknown)
        .put(
            "startStop",
            prefs.putTape(
                JSONObject()
                    .put("mode", prefs.startStopMode)
                    .put("ms", prefs.startStopMs)
                    .put("stopMs", prefs.startStopStopMs)
                    .put("skips", prefs.startStopSkips)
                    .put("rewind", prefs.startStopRewind)
                    .put("edges", prefs.windEdges),
            ),
        )
        // Local mode (decision 0052): which music plays, and whether Android lets the app read the device's.
        .put("mode", prefs.mode)
        // Where this device's controls point (decision 0062): the page shows that, and keeps no output of its own. None
        // set yet (an app from before it): null, and the page says its last choice once.
        .put("target", if (prefs.hasTarget) prefs.target else org.json.JSONObject.NULL)
        .put("deviceMusic", DeviceMusic.state(this))
        // Local mode is there (a folder on, Android's permission given): the switch shows. `localMode` (0.9.7): as
        // `local` it overwrote the local address above, and the page never had it.
        .put("localMode", DeviceMusic.on(this))
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

    /** Android's page of this app: its permissions (the device's music) and its battery use (Background). Android has
     *  no public way to open one permission's page or the app's battery page directly; this is the documented one. */
    private fun openAppSettings() {
        startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
    }

    private fun openOutside(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (e: ActivityNotFoundException) {
            Log.w(TAG, "nothing opens $url")
        }
    }

    /** A page just loaded, perhaps right after the sign-in page (decision 0039): the session it holds now is noted, and
     *  when it is another one, the app's own link to Cratarr connects again with it. The cookies go to disk as well. */
    private fun followSession() {
        lifecycleScope.launch(Dispatchers.IO) {
            try {
                CookieManager.getInstance().flush()
            } catch (e: Exception) {
                // no WebView: no session either
            }
            val mark = Net.sessionMark(this@MainActivity)
            if (mark != prefs.sessionMark) prefs.sessionMark = mark
        }
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
        /** The first page waits this long at most for the address that answers (the local one answers in 1.5 s). */
        const val FIRST_LOOK_MS = 2_000L
    }
}
