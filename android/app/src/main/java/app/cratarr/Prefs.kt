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

    /** The id of the Cratarr the app belongs to (its health check's `instance`): learned at the public address over
     *  https, else the first time one answers; the local address is that Cratarr only when it answers with it. */
    var instance: String?
        get() = p.getString(INSTANCE, null)
        set(value) {
            p.edit().putString(INSTANCE, value).apply()
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
    /** Where this device's controls point (decision 0062): [TARGET_SELF], `player:<name>` (one of Cratarr's players,
     *  driven), or [TARGET_SERVER] (the media server's player). Changed only by the player service. */
    var target: String
        get() = p.getString(TARGET, null)?.takeIf { it == TARGET_SELF || it == TARGET_SERVER || (it.startsWith("player:") && it.length > 7) } ?: TARGET_SELF
        set(value) {
            p.edit().putString(TARGET, value).apply()
        }

    /** Whether a target was ever set here: an app from before decision 0062 takes the page's last choice once. */
    val hasTarget: Boolean get() = p.contains(TARGET)

    /** Why the target last changed, for the page. */
    var targetWhy: String
        get() = p.getString(TARGET_WHY, null).orEmpty()
        set(value) {
            p.edit().putString(TARGET_WHY, value).apply()
        }

    /** The player driven, by name; null when the target is not one of Cratarr's players. */
    val driven: String? get() = target.takeIf { it.startsWith("player:") }?.removePrefix("player:")

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

    /** This device's output as chosen in its sheet (decision 0033, amended 2026-10-19): an output's id, or empty for
     *  Automatic; and which outputs there were when it was chosen (it lasts until they change). */
    var outputChoice: String
        get() = p.getString(OUTPUT_CHOICE, null).orEmpty()
        set(value) {
            p.edit().putString(OUTPUT_CHOICE, value).apply()
        }
    var outputChoiceSet: String
        get() = p.getString(OUTPUT_CHOICE_SET, null).orEmpty()
        set(value) {
            p.edit().putString(OUTPUT_CHOICE_SET, value).apply()
        }

    /** Which music the app plays (decision 0052): Cratarr's ([MODE_CRATARR]) or the device's own ([MODE_DEVICE]). */
    var mode: String
        get() = if (p.getString(MODE, null) == MODE_DEVICE) MODE_DEVICE else MODE_CRATARR
        set(value) {
            p.edit().putString(MODE, if (value == MODE_DEVICE) MODE_DEVICE else MODE_CRATARR).apply()
        }

    /** Local mode's folders (decision 0052, the owner: local mode shows "if the user has enabled any local
     *  directories"): the device's music folders turned on, by their path in Android's index ("Music/"). Local mode
     *  reads only these. */
    var localFolders: Set<String>
        get() = p.getStringSet(LOCAL_FOLDERS, null)?.toSet().orEmpty()
        set(value) {
            p.edit().putStringSet(LOCAL_FOLDERS, value.toSet()).apply()
        }

    /** Android's permission for the device's music was asked for once (a refusal then shows as such). */
    var deviceMusicAsked: Boolean
        get() = p.getBoolean(DEVICE_MUSIC_ASKED, false)
        set(value) {
            p.edit().putBoolean(DEVICE_MUSIC_ASKED, value).apply()
        }

    /** A mode's queue as put by (PlayerService's snapshot, as JSON; empty for none): the one not playing, and local
     *  mode's while it plays too, so it lasts a restart (Cratarr gives its own back). */
    fun savedQueue(mode: String): String = p.getString(if (mode == MODE_DEVICE) DEVICE_QUEUE else CRATARR_QUEUE, null).orEmpty()

    fun saveQueue(mode: String, snapshot: String) {
        p.edit().putString(if (mode == MODE_DEVICE) DEVICE_QUEUE else CRATARR_QUEUE, snapshot).apply()
    }

    /** Automatic follows the owner's order of outputs (decision 0033, amended 2026-10-19: [Output.preferred]); off:
     *  Android's own choice. */
    var outputOrdered: Boolean
        get() = p.getBoolean(OUTPUT_ORDERED, false)
        set(value) {
            p.edit().putBoolean(OUTPUT_ORDERED, value).apply()
        }

    /** Every output seen here, there or not, in the owner's order, as a JSON array of `{id, type, name}` (Output). */
    var outputKnown: String
        get() = p.getString(OUTPUT_KNOWN, null).orEmpty()
        set(value) {
            p.edit().putString(OUTPUT_KNOWN, value).apply()
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

    /** A track levelling has no measure of (0.9.8): [LEVEL_TYPICAL] (turned down as a modern master needs) or
     *  [LEVEL_UNTOUCHED]. */
    var levelUnknown: String
        get() = p.getString(LEVEL_UNKNOWN, null)?.takeIf { it == LEVEL_UNTOUCHED } ?: LEVEL_TYPICAL
        set(value) {
            p.edit().putString(LEVEL_UNKNOWN, if (value == LEVEL_UNTOUCHED) LEVEL_UNTOUCHED else LEVEL_TYPICAL).apply()
        }

    /** Start and stop (0.9.6, Settings › This device; `StartStop`): off, vinyl or fade; over this long; for next and
     *  previous too; Tape rewind on Previous restarting a track. */
    var startStopMode: String
        get() = p.getString(START_STOP_MODE, START_STOP_OFF)?.takeIf { it == START_STOP_VINYL || it == START_STOP_FADE } ?: START_STOP_OFF
        set(value) {
            p.edit().putString(START_STOP_MODE, value).apply()
        }

    var startStopMs: Long
        get() = p.getLong(START_STOP_MS, 800L).coerceIn(200L, 3000L)
        set(value) {
            p.edit().putLong(START_STOP_MS, value.coerceIn(200L, 3000L)).apply()
        }

    /** Start and stop's stop: a pause's wind-down and a skip's (2026-10-27: its own; the tape's Brake is the tape's). */
    var startStopStopMs: Long
        get() = p.getLong(START_STOP_STOP_MS, 800L).coerceIn(200L, 3000L)
        set(value) {
            p.edit().putLong(START_STOP_STOP_MS, value.coerceIn(200L, 3000L)).apply()
        }

    var startStopSkips: Boolean
        get() = p.getBoolean(START_STOP_SKIPS, false)
        set(value) {
            p.edit().putBoolean(START_STOP_SKIPS, value).apply()
        }

    var startStopRewind: Boolean
        get() = p.getBoolean(START_STOP_REWIND, false)
        set(value) {
            p.edit().putBoolean(START_STOP_REWIND, value).apply()
        }

    /**
     * A motion as the players use it (the page's lib/startStop pointWind, holdWind): a wind to a point's length (seconds
     * per minute wound), its speed limit or a hold's starting speed (×), the spin-up and the brake (ms), each piece of a
     * cut wind (ms), how much quieter it winds (dB); a hold's doubling time (s) and top speed (×). Made from the Tape
     * page's settings, never kept.
     */
    data class Wind(
        val ratio: Float,
        val speed: Float,
        val attackMs: Long,
        val releaseMs: Long,
        val pieceMs: Long,
        val gainDb: Float,
        val doublingS: Float,
        val top: Float,
    )

    /** A setting's range and starting value. */
    internal class Range(val min: Float, val max: Float, val start: Float)

    /** The tape's mechanics, alike for every motion; a wind to a point back and forward (duration, speed limit); a hold
     *  back and forward (starting and top speed). */
    private fun tapeSetting(group: String, name: String): Float {
        val r = TAPE_SETTINGS.getValue(group).getValue(name)
        return p.getFloat("tape.$group.$name", r.start).coerceIn(r.min, r.max)
    }

    /** The tape's one brake (a Vinyl pause's too). */
    val tapeBrakeMs: Long get() = tapeSetting("tape", "brakeMs").toLong()

    /** The tape's spin-up. */
    val tapeAttackMs: Long get() = tapeSetting("tape", "attackMs").toLong()

    /** A motion's one setting (the page's `startStop.motions`, 2026-10-28): a wind to a point's duration (s per minute
     *  wound) or a hold's time to its top speed (s); 0: off. */
    fun motion(name: String): Float = tapeSetting("motions", name)

    /** Whether a motion winds the tape: winding on, and it not off. */
    fun windsBy(name: String): Boolean = startStopRewind && motion(name) > 0f

    /** A wind to a point (`toStart` Previous, `toEnd` Next, `seekBack`, `seekFwd`): its duration as set (1 where it is
     *  off but asked for, a page's own seek), at [POINT_SPEED] at most, with the tape's mechanics. */
    fun pointWind(name: String): Wind {
        val ratio = motion(name).takeIf { it > 0f } ?: 1f
        return Wind(
            ratio = ratio,
            speed = POINT_SPEED,
            attackMs = tapeAttackMs,
            releaseMs = tapeBrakeMs,
            pieceMs = PIECE_MS,
            gainDb = tapeSetting("tape", "gainDb"),
            doublingS = 1.5f,
            top = POINT_SPEED,
        )
    }

    /** A hold that way: from [HOLD_START] (at most half its top), doubling to [HOLD_TOP] in its time to top speed after
     *  its spin-up (the page's holdWind). Off, it still gives the tape's mechanics (the knob's spin-up and brake). */
    fun holdWind(back: Boolean, top: Float = HOLD_TOP): Wind {
        val t = motion(if (back) "holdBack" else "holdFwd")
        val start = minOf(HOLD_START, top / 2f)
        val doubling = if (t > 0f) maxOf(0.05f, t - tapeAttackMs / 1000f) / kotlin.math.log2(top / start) else 1.5f
        return pointWind(if (back) "toStart" else "toEnd").copy(speed = start, top = top, doublingS = doubling)
    }

    /** The Tape page's settings as it sends them (`startStop.tape`, `back`, `fwd`, `holdBack`, `holdFwd`): each kept
     *  as it is where left out. */
    fun setTape(startStop: org.json.JSONObject) {
        val e = p.edit()
        for ((group, names) in TAPE_SETTINGS) {
            val o = startStop.optJSONObject(group) ?: continue
            for ((name, r) in names) if (o.has(name)) e.putFloat("tape.$group.$name", o.optDouble(name, r.start.toDouble()).toFloat().coerceIn(r.min, r.max))
        }
        e.apply()
    }

    /** The same, as the page reads it, into [startStop]. */
    fun putTape(startStop: org.json.JSONObject): org.json.JSONObject {
        for ((group, names) in TAPE_SETTINGS) {
            val o = org.json.JSONObject()
            for (name in names.keys) {
                val v = tapeSetting(group, name)
                if (name.endsWith("Ms")) o.put(name, v.toLong()) else o.put(name, v.toDouble())
            }
            startStop.put(group, o)
        }
        return startStop
    }

    /** At a track's edge, holding: stop, or wind on through the queue (`startStop.edges`). */
    var windEdges: String
        get() = if (p.getString(WIND_EDGES, "stop") == "queue") "queue" else "stop"
        set(value) = p.edit().putString(WIND_EDGES, if (value == "queue") "queue" else "stop").apply()

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
        const val LEVEL_UNKNOWN = "level_unknown"
        const val LEVEL_TYPICAL = "typical"
        const val LEVEL_UNTOUCHED = "untouched"
        const val START_STOP_MODE = "start_stop_mode"
        const val START_STOP_MS = "start_stop_ms"
        const val START_STOP_STOP_MS = "start_stop_stop_ms"
        const val START_STOP_SKIPS = "start_stop_skips"
        const val START_STOP_REWIND = "start_stop_rewind"

        /** The Tape page's settings (the owner, 2026-10-27): each one's range and start, as the page's lib/startStop
         *  (and shared/rules.json, which the tests check this against). */
        internal val TAPE_SETTINGS: Map<String, Map<String, Range>> = mapOf(
            "tape" to mapOf(
                "attackMs" to Range(100f, 1500f, 400f),
                "brakeMs" to Range(100f, 1500f, 250f),
                "gainDb" to Range(-24f, 0f, -12f),
            ),
            // Six motions, one setting each, 0: off (2026-10-28).
            "motions" to mapOf(
                "toStart" to Range(0f, 2f, 1f),
                "seekBack" to Range(0f, 2f, 1f),
                "holdBack" to Range(0f, 12f, 4.5f),
                "toEnd" to Range(0f, 2f, 1f),
                "seekFwd" to Range(0f, 2f, 1f),
                "holdFwd" to Range(0f, 12f, 4.5f),
            ),
        )

        /** What is no longer set (the owner, 2026-10-28: good defaults): a wind to a point's speed limit, a hold's
         *  starting and top speed. */
        const val POINT_SPEED = 8f
        /** A wind cut short is heard in pieces this long (ms; 2026-10-28, no longer a setting). */
        const val PIECE_MS = 350L
        /** A hold's starting and top speed (the page's, 2026-10-28: from 4× to 16×, a browser's fastest). */
        const val HOLD_START = 4f
        const val HOLD_TOP = 16f
        /** How fast a hold flies at most (the page's FLIGHT_TOP): heard at [HOLD_TOP] at most, past it in pieces of
         *  [PIECE_MS], each from where the flight has got (the server's `flight_max`). */
        const val FLIGHT_TOP = 128f
        const val WIND_EDGES = "wind_edges"
        const val START_STOP_OFF = "off"
        const val START_STOP_VINYL = "vinyl"
        const val START_STOP_FADE = "fade"
        const val AUTOPLAY = "autoplay"
        private const val OLD_RADIO = "radio"
        const val SLEEP = "sleep"
        const val REPEAT = "repeat"
        const val SHUFFLED = "shuffled"
        const val QUEUE_ID = "queue_id"
        const val OUTPUT_CHOICE = "output_choice"
        const val OUTPUT_CHOICE_SET = "output_choice_set"
        const val OUTPUT_ORDERED = "output_ordered"
        const val OUTPUT_KNOWN = "output_known"
        const val QUEUE_NAME = "queue_name"
        const val MODE = "mode"
        const val MODE_CRATARR = "cratarr"
        const val MODE_DEVICE = "device"
        const val DEVICE_MUSIC_ASKED = "device_music_asked"
        const val LOCAL_FOLDERS = "local_folders"
        const val DEVICE_QUEUE = "device_queue"
        const val CRATARR_QUEUE = "cratarr_queue"
        /** The public address (the app's only one before it had two). */
        const val SERVER_URL = "server_url"
        const val LOCAL_URL = "local_url"
        const val PREFER_LOCAL = "prefer_local"
        private const val OLD_SIGN_IN_USER = "sign_in_user"
        private const val OLD_SIGN_IN_PASSWORD = "sign_in_password"
        const val SESSION_MARK = "session_mark"
        const val IN_USE = "address_in_use"
        const val INSTANCE = "instance"
        const val PAGE_PREFS = "page_prefs"
        const val PLAYER_NAME = "player_name"
        const val STAY_READY = "stay_ready"
        const val TARGET = "target"
        const val TARGET_WHY = "target_why"
        const val TARGET_SELF = "self"
        const val TARGET_SERVER = "server"
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
