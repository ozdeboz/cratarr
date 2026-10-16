package app.cratarr

import android.content.Context
import android.content.SharedPreferences
import android.net.Uri
import android.os.Build

/** The app's few settings, changed from the "This device" section of Cratarr's own Settings page (the bridge). */
class Prefs(context: Context) {
    private val p: SharedPreferences = shared(context)

    /** The Cratarr server's address, with its scheme; null until set. */
    var serverUrl: String?
        get() = p.getString(SERVER_URL, null)?.trim()?.takeIf { it.isNotEmpty() }
        set(value) {
            p.edit().putString(SERVER_URL, value).apply()
        }

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

    /** The radio of before (decision 0036 as first built: one lane after everything, with modes) is Autoplay now. */
    fun migrate() {
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
        const val AUTOPLAY = "autoplay"
        private const val OLD_RADIO = "radio"
        const val SLEEP = "sleep"
        const val REPEAT = "repeat"
        const val SHUFFLED = "shuffled"
        const val SERVER_URL = "server_url"
        const val PLAYER_NAME = "player_name"
        const val STAY_READY = "stay_ready"
        const val DOWNLOAD_QUALITY = "download_quality"
        const val DOWNLOAD_MOBILE = "download_mobile"
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
