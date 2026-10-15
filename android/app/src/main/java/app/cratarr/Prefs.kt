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

    companion object {
        const val SERVER_URL = "server_url"
        const val PLAYER_NAME = "player_name"
        const val STAY_READY = "stay_ready"

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
