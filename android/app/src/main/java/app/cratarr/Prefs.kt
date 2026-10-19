package app.cratarr

import android.content.Context
import android.content.SharedPreferences
import android.net.Uri
import android.os.Build

/** The app's few settings, changed from the "This device" section of Cratarr's own Settings page (the bridge). */
class Prefs(context: Context) {
    private val p: SharedPreferences = shared(context)

    /** Cratarr's public address (the one used from anywhere), with its scheme; null until set. */
    var publicAddress: String?
        get() = p.getString(SERVER_URL, null)?.trim()?.takeIf { it.isNotEmpty() }
        set(value) {
            p.edit().putString(SERVER_URL, value).apply()
        }

    /** Cratarr's local network address (also reached over a VPN that carries only that network); optional. */
    var localAddress: String?
        get() = p.getString(LOCAL_URL, null)?.trim()?.takeIf { it.isNotEmpty() }
        set(value) {
            p.edit().putString(LOCAL_URL, value).apply()
        }

    /** Prefer the local network: its address is used whenever it answers. */
    var preferLocal: Boolean
        get() = p.getBoolean(PREFER_LOCAL, false)
        set(value) {
            p.edit().putBoolean(PREFER_LOCAL, value).apply()
        }

    /** The local network address, when it is to be preferred. */
    val preferredLocal: String? get() = localAddress?.takeIf { preferLocal }

    /** The address in use, which every request goes to: the local network's while preferred and answering, else the
     *  public one ([Reach]). */
    val serverUrl: String?
        get() {
            val local = preferredLocal
            val public = publicAddress
            return p.getString(IN_USE, null)?.takeIf { it == local || it == public } ?: public ?: local
        }

    /** The address in use as last chosen (null before any was). */
    val inUse: String? get() = p.getString(IN_USE, null)

    /** Uses [address] (one of the two) from now on. */
    fun use(address: String?) {
        p.edit().putString(IN_USE, address).apply()
    }

    /** [from], one of the two addresses, sends everything on to [to] for good: [to] is that address now (in use too). */
    fun moved(from: String, to: String) {
        val e = p.edit()
        if (publicAddress == from) e.putString(SERVER_URL, to)
        if (localAddress == from) e.putString(LOCAL_URL, to)
        if (inUse == from) e.putString(IN_USE, to)
        e.apply()
    }

    /** The session on the public address as last seen ([Net.sessionMark], never the cookie): when the page signs in
     *  again, the app's own link to Cratarr connects again with it. */
    var sessionMark: String
        get() = p.getString(SESSION_MARK, "") ?: ""
        set(value) {
            p.edit().putString(SESSION_MARK, value).apply()
        }

    /** The page's own settings (its `cratarr-prefs`), kept here so both addresses' pages share them. */
    var pagePrefs: String?
        get() = p.getString(PAGE_PREFS, null)
        set(value) {
            p.edit().putString(PAGE_PREFS, value).apply()
        }

    /** The origins of the two addresses: the pages that stay in the app and may talk to it. */
    fun origins(): Set<String> = listOfNotNull(localAddress, publicAddress).mapNotNull(::originOf).toSet()

    /** How Home Assistant and the other players see this one; the device's model until set. */
    var playerName: String
        get() = p.getString(PLAYER_NAME, null)?.trim()?.takeIf { it.isNotEmpty() } ?: Build.MODEL
        set(value) {
            p.edit().putString(PLAYER_NAME, value.trim()).apply()
        }

    /** Kept awake and listening while idle too: for a device that is always on its charger. */
    var stayReady: Boolean
        get() = p.getBoolean(STAY_READY, false)
        set(value) {
            p.edit().putBoolean(STAY_READY, value).apply()
        }

    /** Downloads as the files are ([QUALITY_ORIGINAL]) or as Cratarr's smaller copies ([QUALITY_SMALLER]). */
    var downloadQuality: String
        get() = p.getString(DOWNLOAD_QUALITY, QUALITY_ORIGINAL) ?: QUALITY_ORIGINAL
        set(value) {
            p.edit().putString(DOWNLOAD_QUALITY, if (value == QUALITY_SMALLER) QUALITY_SMALLER else QUALITY_ORIGINAL).apply()
        }

    /** Downloads over mobile data too, not only over Wi-Fi. */
    var downloadOverMobile: Boolean
        get() = p.getBoolean(DOWNLOAD_MOBILE, false)
        set(value) {
            p.edit().putBoolean(DOWNLOAD_MOBILE, value).apply()
        }

    /** Kept on this device (decision 0038): the storage budget in GB, 0 for no limit. */
    var syncBudgetGb: Long
        get() = p.getLong(SYNC_BUDGET_GB, 20L)
        set(value) {
            p.edit().putLong(SYNC_BUDGET_GB, value.coerceAtLeast(0L)).apply()
        }

    /** Downloads (kept and the owner's) only while the device charges. */
    var syncOnlyCharging: Boolean
        get() = p.getBoolean(SYNC_ONLY_CHARGING, false)
        set(value) {
            p.edit().putBoolean(SYNC_ONLY_CHARGING, value).apply()
        }

    /** Repeat: off, one or all (Repeat). */
    var repeat: String
        get() = p.getString(REPEAT, Repeat.OFF) ?: Repeat.OFF
        set(value) {
            p.edit().putString(REPEAT, value).apply()
        }

    /** The queue plays shuffled: repeat all shuffles each new round. */
    var shuffled: Boolean
        get() = p.getBoolean(SHUFFLED, false)
        set(value) {
            p.edit().putBoolean(SHUFFLED, value).apply()
        }

    /** Which queue plays, and its name (decision 0033, amended: queue history): Cratarr's for a play it sends, a new one
     *  for a play the page starts here. Reported with every report; Cratarr keeps the queue before a new one. */
    var queueId: String
        get() = p.getString(QUEUE_ID, null).orEmpty()
        set(value) {
            p.edit().putString(QUEUE_ID, value).apply()
        }
    var queueName: String
        get() = p.getString(QUEUE_NAME, null).orEmpty()
        set(value) {
            p.edit().putString(QUEUE_NAME, value).apply()
        }

    /** A new queue starts: its own id, and its name (empty: Cratarr names it after its album, artist or first track). */
    fun newQueue(name: String) {
        p.edit().putString(QUEUE_ID, java.util.UUID.randomUUID().toString()).putString(QUEUE_NAME, name).apply()
    }

    /** The sleep timer (Sleep), as "atMs,endOfTrack" (no time: empty); null when none is set. */
    var sleep: Sleep.Timer?
        get() = p.getString(SLEEP, null)?.split(',')?.takeIf { it.size == 2 }?.let { (at, end) -> Sleep.Timer(at.toLongOrNull(), end == "true") }
        set(value) {
            p.edit().apply { if (value == null) remove(SLEEP) else putString(SLEEP, "${value.atMs ?: ""},${value.endOfTrack}") }.apply()
        }

    /** Radio inserts (Radio): on or off. */
    var radioOn: Boolean
        get() = p.getBoolean(RADIO_ON, false)
        set(value) {
            p.edit().putBoolean(RADIO_ON, value).apply()
        }

    /** What Radio's tracks are like: the track, its artist or its era (decision 0036's choice). */
    var radioBy: String
        get() = p.getString(RADIO_BY, null)?.takeIf { it in Radio.BYS } ?: Radio.BYS[0]
        set(value) {
            p.edit().putString(RADIO_BY, value.takeIf { it in Radio.BYS } ?: Radio.BYS[0]).apply()
        }

    /** How many Radio inserts after each of the queue's own tracks. */
    var radioCount: Int
        get() = p.getInt(RADIO_COUNT, Radio.COUNT).coerceIn(1, Radio.MAX)
        set(value) {
            p.edit().putInt(RADIO_COUNT, value.coerceIn(1, Radio.MAX)).apply()
        }

    /** Levelling (decision 0035): each track about as loud as the next, by its ReplayGain. On unless turned off. */
    var levelling: Boolean
        get() = p.getBoolean(LEVELLING, true)
        set(value) {
            p.edit().putBoolean(LEVELLING, value).apply()
        }

    /** Ignore favourites (SkipFavourites): Radio's inserts and Autoplay's picks leave the hearted tracks out. */
    var skipFavourites: Boolean
        get() = p.getBoolean(SKIP_FAVOURITES, false)
        set(value) {
            p.edit().putBoolean(SKIP_FAVOURITES, value).apply()
        }

    /** Autoplay (Autoplay), as `{"mode", "albums", "restart", "seed", "within"}`; null while it is off. */
    var autoplay: Autoplay.State?
        get() = p.getString(AUTOPLAY, null)?.let {
            try {
                val o = org.json.JSONObject(it)
                val seed = o.optJSONArray("seed")
                Autoplay.State(
                    o.optString("mode", "similar"),
                    o.optBoolean("albums"),
                    o.optBoolean("restart"),
                    (0 until (seed?.length() ?: 0)).map { i -> seed!!.optString(i) },
                    o.optString("within").takeIf { w -> w.isNotEmpty() && !o.isNull("within") },
                )
            } catch (e: Exception) {
                null
            }
        }
        set(value) {
            p.edit().apply {
                if (value == null) remove(AUTOPLAY)
                else putString(
                    AUTOPLAY,
                    org.json.JSONObject()
                        .put("mode", value.mode)
                        .put("albums", value.albums)
                        .put("restart", value.restart)
                        .put("seed", org.json.JSONArray(value.seed))
                        .put("within", value.within ?: org.json.JSONObject.NULL)
                        .toString(),
                )
            }.apply()
        }

    /** Settings of before. The sign-in kept for the proxy's login box goes: the app keeps no password since Cratarr has
     *  its own sign-in page (decision 0039). The radio (decision 0036 as first built: one lane after everything, with
     *  modes) is Autoplay now. */
    fun migrate() {
        if (p.contains(OLD_SIGN_IN_USER) || p.contains(OLD_SIGN_IN_PASSWORD)) p.edit().remove(OLD_SIGN_IN_USER).remove(OLD_SIGN_IN_PASSWORD).apply()
        val old = p.getString(OLD_RADIO, null) ?: return
        try {
            val o = org.json.JSONObject(old)
            val mode = o.optString("mode", "similar")
            val seed = o.optJSONArray("seed")
            autoplay = Autoplay.State(
                if (mode in listOf("continuous", "album_mix", "reshuffle")) "similar" else mode,
                mode == "album_mix",
                mode == "reshuffle",
                (0 until (seed?.length() ?: 0)).map { seed!!.optString(it) },
                o.optString("within").takeIf { it.isNotEmpty() && !o.isNull("within") },
            )
        } catch (e: Exception) {
            // nothing usable: off
        }
        p.edit().remove(OLD_RADIO).apply()
    }

    companion object {
        const val RADIO_ON = "radio_on"
        const val RADIO_BY = "radio_by"
        const val RADIO_COUNT = "radio_count"
        const val SKIP_FAVOURITES = "skip_favourites"
        const val LEVELLING = "levelling"
        const val AUTOPLAY = "autoplay"
        private const val OLD_RADIO = "radio"
        const val SLEEP = "sleep"
        const val REPEAT = "repeat"
        const val SHUFFLED = "shuffled"
        const val QUEUE_ID = "queue_id"
        const val QUEUE_NAME = "queue_name"
        /** The public address (the app's only one before it had two). */
        const val SERVER_URL = "server_url"
        const val LOCAL_URL = "local_url"
        const val PREFER_LOCAL = "prefer_local"
        private const val OLD_SIGN_IN_USER = "sign_in_user"
        private const val OLD_SIGN_IN_PASSWORD = "sign_in_password"
        const val SESSION_MARK = "session_mark"
        const val IN_USE = "address_in_use"
        const val PAGE_PREFS = "page_prefs"
        const val PLAYER_NAME = "player_name"
        const val STAY_READY = "stay_ready"
        const val DOWNLOAD_QUALITY = "download_quality"
        const val DOWNLOAD_MOBILE = "download_mobile"
        const val SYNC_BUDGET_GB = "sync_budget_gb"
        const val SYNC_ONLY_CHARGING = "sync_only_charging"
        const val QUALITY_ORIGINAL = "original"
        const val QUALITY_SMALLER = "smaller"

        /** The settings file (the one Android's preference screens used, so settings made before carry over). */
        fun shared(context: Context): SharedPreferences =
            context.getSharedPreferences("${context.packageName}_preferences", Context.MODE_PRIVATE)

        /** An address as typed: with a scheme (http when none), without a trailing slash. */
        fun normalise(typed: String): String {
            val t = typed.trim().trimEnd('/')
            return if ("://" in t) t else "http://$t"
        }

        /** `scheme://host[:port]` of an address: the only origin whose pages may talk to the app. */
        fun originOf(url: String): String? {
            val u = Uri.parse(url)
            val scheme = u.scheme?.lowercase() ?: return null
            val host = u.host?.lowercase() ?: return null
            return if (u.port == -1) "$scheme://$host" else "$scheme://$host:${u.port}"
        }
    }
}
