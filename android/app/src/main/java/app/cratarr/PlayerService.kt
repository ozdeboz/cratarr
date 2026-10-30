/*
 * Changed for Cratarr, 2026-10-15: in the place of Jellyfin's RemotePlayerService (jellyfin-android, GPL-2.0), which
 * showed the page's playback on the lock screen. Here the app plays itself (decision 0031, amended): Media3's player in
 * a media session service, which brings the lock screen, the notification, headphone buttons, audio focus, pausing
 * when the audio would become noisy, and the wake and Wi-Fi locks while it plays. It also holds this player's Play On
 * connection to Cratarr (PlayOnLink); with Stay ready the app stays in the foreground by a service of its own (Ready).
 * Tracks are read through the device's store (Downloads): a downloaded one, or one of the next few, plays without a
 * connection. In local mode (decision 0052) the same player plays the device's own music (DeviceMusic), with a queue of
 * its own.
 */
package app.cratarr

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.view.KeyEvent
import android.content.SharedPreferences
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.CacheBitmapLoader
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import org.json.JSONException
import org.json.JSONObject

@OptIn(UnstableApi::class)
class PlayerService : MediaSessionService() {
    private var session: MediaSession? = null
    private val main = Handler(Looper.getMainLooper())
    private var link: PlayOnLink? = null
    private lateinit var prefs: Prefs
    private lateinit var notifications: NotificationManager
    private lateinit var player: ExoPlayer
    /** Sync play (decision 0037): this device in a group. */
    private lateinit var sync: SyncPlay

    /** Start and stop (0.9.6): play, pause and stop, and next and previous where asked, ramped as the device says. */
    lateinit var startStop: StartStop

    /** Who listens to the session's player (the session itself): told when a ramp commits to a state. */
    private val sessionListeners = java.util.concurrent.CopyOnWriteArraySet<Player.Listener>()

    /**
     * The track's own pauses and plays as the session shows them: what a ramp (or a wind) has committed to while one
     * runs. A hold pauses the track under it (the owner, 2026-10-27: "the play pause button flashes"), and the lock
     * screen, the notification and the page heard that pause before; they see playing throughout.
     */
    private inner class Shown(private val l: Player.Listener) : Player.Listener by l {
        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) = l.onPlayWhenReadyChanged(startStop.committed ?: playWhenReady, reason)

        override fun onIsPlayingChanged(isPlaying: Boolean) = l.onIsPlayingChanged(startStop.committed ?: isPlaying)
    }

    private val shown = java.util.concurrent.ConcurrentHashMap<Player.Listener, Shown>()

    /** A ramp committed to playing or paused, or let go: the session (lock screen, notification, the page) and Cratarr
     *  hear the state it shows. */
    private fun startStopChanged() {
        val events = Player.Events(androidx.media3.common.FlagSet.Builder().addAll(Player.EVENT_PLAY_WHEN_READY_CHANGED, Player.EVENT_IS_PLAYING_CHANGED).build())
        for (l in sessionListeners) {
            l.onPlayWhenReadyChanged(sessionPlayer.playWhenReady, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            l.onIsPlayingChanged(sessionPlayer.isPlaying)
            l.onEvents(sessionPlayer, events)
        }
        link?.startStopChanged()
    }
    /** The app's own player as the session shows it (its previous and shuffle the app's way). */
    private lateinit var sessionPlayer: Player
    /** The player the screen drives, as the session shows it meanwhile (the owner, 2026-10-23). */
    private lateinit var remote: RemotePlayer
    /** The player's commands: Play On's and the page's, one way in. */
    private lateinit var commands: PlayerCommands
    /** A track's snippet beside the music, which it ducks by volume (0.12, snippets: the owner chose them, 2026-10-30). */
    private lateinit var snippets: Snippets

    /** The server's address or the player's name changed: Play On connects again with them. A sleep timer set: kept. */
    private val prefsChanged = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        when (key) {
            Prefs.IN_USE -> link?.reconnect("address changed")
            Prefs.PLAYER_NAME -> link?.reconnect("name changed")
            Prefs.STAY_READY -> {
                link?.reconnect("stay ready changed")
                Ready.ensure(this, "stay ready changed")
            }
            Prefs.SESSION_MARK -> link?.reconnect("session changed")
        }
        // An address set: which of the two answers.
        if (key == Prefs.SERVER_URL || key == Prefs.LOCAL_URL || key == Prefs.PREFER_LOCAL || key == Prefs.SESSION_MARK) Reach.checkSoon(this)
        // Another server or name: Cratarr says the group again once connected.
        if ((key == Prefs.SERVER_URL || key == Prefs.LOCAL_URL || key == Prefs.PLAYER_NAME) && sync.leader != null) sync.group(null, null, 0, sync.bufferMs, "")
        if (key == Prefs.LEVELLING || key == Prefs.LEVEL_UNKNOWN) link?.levellingChanged()
        if (key == Prefs.TARGET) showTarget()
        if (key == Prefs.OUTPUT_CHOICE || key == Prefs.OUTPUT_ORDERED || key == Prefs.OUTPUT_KNOWN) applyOutput()
        if (key == Prefs.SLEEP) {
            keepSleep()
            link?.sleepChanged()
        }
        // Radio or Autoplay set (here, by the page or by Cratarr): Cratarr is told; the playing track gets its inserts,
        // the queue its next ones when it runs low.
        if (key == Prefs.RADIO_ON || key == Prefs.RADIO_BY || key == Prefs.RADIO_COUNT || key == Prefs.AUTOPLAY || key == Prefs.SKIP_FAVOURITES) {
            link?.sleepChanged()
            Radio.insert(this, player, prefs.serverUrl, ::located)
            Autoplay.fill(this, player, prefs.serverUrl, ::located)
        }
    }

    /** The sleep timer's time, looked at again when it should have come (the device may have slept meanwhile). */
    private val sleepAt = Runnable { sleepCheck() }

    override fun onCreate() {
        super.onCreate()
        // Its own log (decision 0045): the service's start, and a crash, written down.
        LinkLog.catchCrashes(this)
        LinkLog.add(this, "info", "playon", "service started")
        prefs = Prefs(this)
        RustRules.set(this, prefs.rustRules)
        notifications = getSystemService(NotificationManager::class.java)
        val music = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        sync = SyncPlay(this, prefs)
        // Speed on Android's own output (0.9.6, Start and stop's Vinyl): changed smoothly and at once.
        player = ExoPlayer.Builder(this, sync.renderers(this).setEnableAudioTrackPlaybackParams(true))
            .setAudioAttributes(music, true)
            .setHandleAudioBecomingNoisy(true)
            // A wake lock and a Wi-Fi lock while it plays: the screen off stops neither the music nor its download.
            .setWakeMode(C.WAKE_MODE_NETWORK)
            // Previous restarts the track after this long in, as the page's player does (Finamp: 5 s).
            .setMaxSeekToPreviousPositionMs(RESTART_AFTER_MS)
            // Through the device's store first: downloads and the next tracks play from there. The device's own files
            // (local mode, content://) through Android.
            .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(this, Downloads.playback(this))).setLoadErrorHandlingPolicy(Patience()))
            .build()
        player.addListener(Keeper(player))
        player.addListener(Course(player))
        sync.attach(player)
        sync.onChange = { main.post(::groupChanged) }
        Repeat.apply(this, player)
        prefs.migrate()
        // A sleep timer from before: its music is gone.
        Sleep.clear(this)
        // Previous from the lock screen, the notification and the page goes through here too: under repeat all, the
        // first track's previous is the last one.
        // Shuffle is once (decision 0059: a queue always has an order, shuffle is an action, not a state): a car's, a
        // headset's or the system's shuffle shuffles what is to come once, and the player never plays shuffled, so it
        // says it is not. Cratarr's queue is shuffled by Cratarr (Play On's `shuffle`, which sends the new order back);
        // local mode's here, the playing track staying where it is.
        startStop = StartStop(this, player, prefs, { link?.applyVolume() }, { sync.leader != null }, { sync.bufferMs }, ::startStopChanged, ::windUrl, ::windWay, ::brakeUrl, sync) { what, fields ->
            LinkLog.add(this, "info", "startstop", what, *fields)
        }
        sessionPlayer = object : ForwardingPlayer(player) {
            // A ramp under way shows where it is going (0.9.6): the session's listeners told when that changes.
            override fun addListener(listener: Player.Listener) {
                sessionListeners.add(listener)
                super.addListener(shown.getOrPut(listener) { Shown(listener) })
            }

            override fun removeListener(listener: Player.Listener) {
                sessionListeners.remove(listener)
                shown.remove(listener)?.let { super.removeListener(it) }
            }

            override fun getPlayWhenReady() = startStop.committed ?: super.getPlayWhenReady()

            override fun isPlaying() = startStop.committed ?: super.isPlaying()

            // The deck's one truth (decision 0075): Start and stop's held place through its ramps, a wind's place as it
            // winds (in the track the session shows; one wound on into another shows the one it began in).
            override fun getCurrentPosition() = shownPosition() ?: super.getCurrentPosition()

            override fun getContentPosition() = shownPosition() ?: super.getContentPosition()

            private fun shownPosition(): Long? {
                if (!StartStop.winding) return startStop.heldPosition
                val d = startStop.deck()
                return if (d.index == super.getCurrentMediaItemIndex()) d.ms else null
            }

            // The lock screen's, the notification's, a headset's and the page's play, pause and stop, ramped (0.9.6).
            override fun play() = startStop.play { super.play() }

            override fun pause() = startStop.pause { super.pause() }

            override fun setPlayWhenReady(playWhenReady: Boolean) {
                if (playWhenReady) play() else pause()
            }

            override fun stop() = startStop.pause { super.stop() }

            // A seek in the middle of a Start and stop ramp ends it at once (0.9.7, `StartStop.seek`).
            // Where seeking winds the tape (decision 0074), wound there.
            override fun seekTo(positionMs: Long) = startStop.seekTo(positionMs) { super.seekTo(positionMs) }

            // Within the playing track (as a controller may send a seek), winding as the other; another track jumps.
            override fun seekTo(mediaItemIndex: Int, positionMs: Long) =
                if (mediaItemIndex == player.currentMediaItemIndex) startStop.seekTo(positionMs) { super.seekTo(mediaItemIndex, positionMs) }
                else startStop.seek { super.seekTo(mediaItemIndex, positionMs) }

            // Next winds on to the track's end first where winding is on (0.9.14, the mirror of Previous's rewind).
            override fun seekToNext() {
                val jump = { super.seekToNext() }
                if (!startStop.fastForward(jump)) startStop.skip(jump)
            }

            override fun seekToNextMediaItem() {
                val jump = { super.seekToNextMediaItem() }
                if (!startStop.fastForward(jump)) startStop.skip(jump)
            }

            override fun seekToPreviousMediaItem() = startStop.skip { super.seekToPreviousMediaItem() }

            override fun seekToPrevious() {
                val restart = { startStop.skip { if (!Repeat.previous(this@PlayerService, player)) super.seekToPrevious() } }
                if (!startStop.rewind(RESTART_AFTER_MS, restart)) restart()
            }

            override fun setShuffleModeEnabled(shuffleModeEnabled: Boolean) {
                if (!shuffleModeEnabled) return
                if (prefs.mode == Prefs.MODE_DEVICE) shuffleUpcoming(player) else link?.shuffleOnce()
            }

            override fun getShuffleModeEnabled() = false
        }
        commands = PlayerCommands(this, player, sessionPlayer, prefs, startStop) { link }
        snippets = Snippets(this, prefs) { link?.applyVolume() }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        session = MediaSession.Builder(this, sessionPlayer)
            .setSessionActivity(open)
            .setCallback(Callback())
            // Covers (the notification's, the lock screen's, a car's) through the app's own requests, with its session:
            // away from home the proxy refused Media3's own without it (2026-10-19). The device's own albums' from Android.
            .setBitmapLoader(
                CacheBitmapLoader(
                    DeviceMusic.Covers(this, DataSourceBitmapLoader.Builder(this).setDataSourceFactory(DefaultDataSource.Factory(this, Downloads.http(this))).build()),
                ),
            )
            .build()
        remote = RemotePlayer(this, prefs)
        // Local mode (decision 0052) as it was: its queue again, paused; without Android's permission or a folder turned
        // on any more, Cratarr's.
        if (prefs.mode == Prefs.MODE_DEVICE) {
            if (DeviceMusic.on(this)) restore(prefs.savedQueue(Prefs.MODE_DEVICE)) else prefs.mode = Prefs.MODE_CRATARR
        }
        link = PlayOnLink(this, player, prefs, sync, commands).also { it.start() }
        Ready.ensure(this, "service started")
        applyOutput()
        Reach.watch(this)
        Prefs.shared(this).registerOnSharedPreferenceChangeListener(prefsChanged)
        // Where the controls point, as kept.
        showTarget()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_READY) {
            // The app in front: ready (again), and what Android held back plays now.
            Ready.ensure(this, "app opened")
            link?.playHeld()
            // Not ready, nothing queued: nothing keeps it.
            if (!prefs.stayReady && player.mediaItemCount == 0 && prefs.driven == null) stopSelf()
            return START_STICKY
        }
        return super.onStartCommand(intent, flags, startId)
    }

    /**
     * Stay ready: a paused queue's notification in the foreground too; with nothing queued, the Ready service's own
     * notification is the one there is.
     */
    override fun onUpdateNotification(session: MediaSession, startInForegroundRequired: Boolean) {
        // Following a group: its own notification, which keeps the app (and Snapcast's player in it) running.
        if (sync.follows) {
            showGroup()
            notifications.cancel(DefaultMediaNotificationProvider.DEFAULT_NOTIFICATION_ID)
            return
        }
        notifications.cancel(GROUP_NOTIFICATION)
        super.onUpdateNotification(session, startInForegroundRequired || (prefs.stayReady && session.player.mediaItemCount > 0))
    }

    /**
     * Where this device's controls point (decision 0062), changed here only: `self` or `player:<name>` (0.12: the media
     * server's player is gone from Cratarr's mode, the owner 2026-10-30; a page still asking for it gets `self`). Asked by
     * the page, or by the rule: *sounding here means controls here* (its own player started, whatever started it). To
     * point away it pauses its own player first, if it still plays. The page hears every change (MainActivity).
     */
    fun pointAt(to: String, why: String) {
        val target = when {
            to.startsWith("player:") && to.length > 7 && to.removePrefix("player:") != prefs.playerName -> to
            else -> Prefs.TARGET_SELF
        }
        if (target != Prefs.TARGET_SELF && player.isPlaying) {
            LinkLog.add(this, "info", "playon", "own player paused", "why" to "pointing away", "to" to target)
            player.pause()
        }
        if (prefs.target == target) return
        LinkLog.add(this, "info", "playon", "target", "to" to target, "was" to prefs.target, "why" to why)
        prefs.targetWhy = why
        prefs.target = target
    }

    /**
     * Android's media controls (the notification, the lock screen, the output chip, the volume keys) show and control
     * where the target is: one of Cratarr's players through a [RemotePlayer], else this device's own player (the owner,
     * 2026-10-23: "This phone" and a paused track showed while the phone drove the Salon tablet).
     */
    private fun showTarget() {
        val name = prefs.driven
        remote.follow(name)
        val s = session ?: return
        val want: Player = if (name != null) remote else sessionPlayer
        if (s.player !== want) s.player = want
    }

    /** Following a group began or ended. */
    private fun groupChanged() {
        if (sync.follows) showGroup() else session?.let { onUpdateNotification(it, false) }
    }

    private fun showGroup() {
        notifications.createNotificationChannel(NotificationChannel(READY_CHANNEL, getString(R.string.ready), NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val grouped = Notification.Builder(this, READY_CHANNEL)
            .setSmallIcon(R.drawable.media3_notification_small_icon)
            .setContentTitle(getString(R.string.grouped, sync.leader ?: ""))
            .setContentText(prefs.playerName)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        try {
            startForeground(GROUP_NOTIFICATION, grouped, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } catch (e: IllegalStateException) {
            Log.w(TAG, "cannot stay in front while grouped: ${e.message}")
        }
    }

    /**
     * Local mode (decision 0052): Cratarr's music or the device's own, never mixed. Each keeps its queue: the one left
     * (its tracks, the one that plays and where, shuffle and repeat) is put by, and the other one taken up where it was,
     * paused. Entering local mode leaves a group. True when the mode changed.
     */
    fun switchMode(to: String, why: String): Boolean {
        val mode = if (to == Prefs.MODE_DEVICE) Prefs.MODE_DEVICE else Prefs.MODE_CRATARR
        val was = prefs.mode
        if (was == mode || (mode == Prefs.MODE_DEVICE && !DeviceMusic.on(this))) return false
        LinkLog.add(this, "info", "playon", "mode", "to" to mode, "why" to why)
        if (mode == Prefs.MODE_DEVICE && sync.leader != null) sync.leave()
        player.pause()
        prefs.saveQueue(was, snapshot().toString())
        prefs.mode = mode
        restore(prefs.savedQueue(mode))
        link?.modeChanged()
        return true
    }

    /** The queue as it is now, for [restore]. */
    private fun snapshot(): JSONObject = JSONObject()
        .put("tracks", queueOf(player))
        .put("index", player.currentMediaItemIndex)
        .put("position_ms", player.currentPosition)
        .put("repeat", prefs.repeat)
        .put("shuffled", prefs.shuffled)

    /** A queue put by ([snapshot]) in the player, paused; none (empty): nothing queued. */
    private fun restore(saved: String) {
        val s = try {
            JSONObject(saved)
        } catch (e: JSONException) {
            JSONObject()
        }
        prefs.repeat = s.optString("repeat", Repeat.OFF)
        prefs.shuffled = s.optBoolean("shuffled")
        Repeat.apply(this, player)
        val items = mediaItems(s.optJSONArray("tracks")).map(::located)
        if (items.isEmpty()) {
            player.stop()
            player.clearMediaItems()
            return
        }
        player.setMediaItems(items, s.optInt("index").coerceIn(0, items.size - 1), s.optLong("position_ms").coerceAtLeast(0))
        player.prepare()
    }

    /** Local mode's queue kept as it goes, for a restart (changes close together as one). */
    private val saveDevice = Runnable { if (prefs.mode == Prefs.MODE_DEVICE) prefs.saveQueue(Prefs.MODE_DEVICE, snapshot().toString()) }

    /**
     * This device's output as chosen in its sheet (decision 0033, amended 2026-10-19): the player's sound goes there,
     * alone or leading a group (Snapcast's own player, following one, takes none: Android routes it). A choice lasts
     * until the outputs there are change (a Bluetooth device comes or goes): then it is Automatic again, so the speaker
     * picked in the car is not picked the next time.
     */
    fun outputsChanged() {
        if (prefs.outputChoice.isNotEmpty() && Output.set(this) != prefs.outputChoiceSet) {
            LinkLog.add(this, "info", "playon", "output choice ended", "was" to prefs.outputChoice)
            prefs.outputChoice = ""
        }
        applyOutput()
    }

    /** The chosen output, else the first of the owner's order that is there ([Output.preferred]); picked again as
     *  outputs come and go. */
    private fun applyOutput() {
        player.setPreferredAudioDevice(Output.preferred(this))
    }

    /** The sleep timer as set: the end of the track by the player itself, the time by a wait. */
    private fun keepSleep() {
        val timer = Sleep.get(this)
        player.pauseAtEndOfMediaItems = timer?.endOfTrack == true
        main.removeCallbacks(sleepAt)
        timer?.atMs?.let { main.postDelayed(sleepAt, (it - System.currentTimeMillis()).coerceAtLeast(0)) }
    }

    private fun sleepCheck() {
        val at = Sleep.get(this)?.atMs ?: return
        val left = at - System.currentTimeMillis()
        if (left > 0) {
            main.postDelayed(sleepAt, left)
            return
        }
        // Cleared first, so what the pause reports has no timer.
        Sleep.clear(this)
        player.pause()
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        LinkLog.add(this, "info", "playon", "app swiped away")
        // The app swiped away: music that plays goes on, and so does a player that stays ready; otherwise it goes.
        if (!prefs.stayReady && prefs.driven == null && (!player.playWhenReady || player.mediaItemCount == 0)) stopSelf()
    }

    /**
     * The player's own course written down (2026-10-28, the tablet: a track played 160 s past its end, and a hold landed
     * past the queue's last track's end, neither in the log): each change of track and why, its end, and playing on past
     * a track's length (once a track).
     */
    private inner class Course(private val player: ExoPlayer) : Player.Listener {
        private var pastTold = -1

        private fun log(level: String, what: String, vararg fields: Pair<String, Any?>) =
            LinkLog.add(this@PlayerService, level, "player", what, "index" to player.currentMediaItemIndex, "at_ms" to player.currentPosition, *fields)

        private val check = object : Runnable {
            override fun run() {
                val d = player.duration
                val i = player.currentMediaItemIndex
                if (player.isPlaying && d != C.TIME_UNSET && player.currentPosition > d + PAST_END_MS && pastTold != i) {
                    pastTold = i
                    log("warn", "past its end", "length_ms" to d, "count" to player.mediaItemCount, "state" to player.playbackState)
                }
                main.removeCallbacks(this)
                if (player.isPlaying) main.postDelayed(this, PAST_CHECK_MS)
            }
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            pastTold = -1
            log("info", "track changed", "reason" to reason, "count" to player.mediaItemCount, "length_ms" to player.duration.takeIf { it != C.TIME_UNSET })
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_ENDED) log("info", "ended", "count" to player.mediaItemCount, "length_ms" to player.duration.takeIf { it != C.TIME_UNSET })
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (isPlaying) check.run()
        }
    }

    /**
     * Keeps the next tracks on the device as the queue moves on, counts the plays (Plays), and skips a track that cannot
     * be played.
     */
    private inner class Keeper(private val player: ExoPlayer) : Player.Listener {
        private val keep = Runnable {
            // Local mode's tracks are on the device already.
            if (prefs.mode == Prefs.MODE_DEVICE) return@Runnable
            // What the notification and the lock screen show, as Cratarr describes it (0.9.6).
            Describe.fill(this@PlayerService, player)
            val items = (0 until player.mediaItemCount).map(player::getMediaItemAt)
            Downloads.keepAhead(this@PlayerService, items, player.currentMediaItemIndex)
        }

        /**
         * The track being heard (decision 0064): which (as the page describes it), since when, how far it got (ms), how
         * long it is, and whether it was told as counted, this time it plays. The device's own music is not told
         * (decision 0052).
         */
        private inner class Hearing(val track: JSONObject, val atMs: Long) {
            var heardMs = 0L
            /** What winding passed over (decision 0074: no wind counts as listening): the place less this is heard. */
            var offsetMs = 0L
            var lengthMs = 0L
            var counted = false
        }
        private var hearing: Hearing? = null

        /** Tells the track left, unless it was told as counted or heard for less than a second. */
        private fun leave(ended: String) {
            val h = hearing ?: return
            hearing = null
            if (!h.counted && h.heardMs >= 1_000) Plays.heard(this@PlayerService, h.track, h.atMs, ended, false, h.heardMs, h.lengthMs)
        }

        private val count = object : Runnable {
            override fun run() {
                if (hearing == null) {
                    hearing = player.currentMediaItem?.takeUnless(DeviceMusic::owns)?.let(::trackOf)?.let { Hearing(it, System.currentTimeMillis()) }
                }
                val h = hearing
                val d = player.duration
                if (h != null) {
                    // A wind landed: heard carries on from what was heard, not from where it landed.
                    StartStop.landed?.let { at ->
                        StartStop.landed = null
                        h.offsetMs = at - h.heardMs
                    }
                    val heard = player.currentPosition - h.offsetMs
                    h.heardMs = maxOf(h.heardMs, heard)
                    if (d > 0) h.lengthMs = d
                    if (!h.counted && d > 0 && heard >= d * Plays.PLAYED_AT) {
                        h.counted = true
                        Plays.heard(this@PlayerService, h.track, h.atMs, "finished", true, h.heardMs, h.lengthMs)
                    }
                }
                if (player.isPlaying) main.postDelayed(this, COUNT_EVERY_MS)
            }
        }

        override fun onEvents(player: Player, events: Player.Events) {
            if (events.containsAny(Player.EVENT_TIMELINE_CHANGED, Player.EVENT_MEDIA_ITEM_TRANSITION)) {
                // Changes close together (a queue being built) as one.
                main.removeCallbacks(keep)
                main.postDelayed(keep, 1_000)
                // Repeat all: the last track plays, the next round goes after it.
                Repeat.extend(this@PlayerService, player, ::located)
                // Radio: the playing own track's inserts; Autoplay: fewer than three ahead, the next ones (decision 0036).
                Radio.insert(this@PlayerService, player, prefs.serverUrl, ::located)
                Autoplay.fill(this@PlayerService, player, prefs.serverUrl, ::located)
            }
            if (prefs.mode == Prefs.MODE_DEVICE && events.containsAny(
                    Player.EVENT_TIMELINE_CHANGED,
                    Player.EVENT_MEDIA_ITEM_TRANSITION,
                    Player.EVENT_PLAY_WHEN_READY_CHANGED,
                    Player.EVENT_POSITION_DISCONTINUITY,
                )
            ) {
                main.removeCallbacks(saveDevice)
                main.postDelayed(saveDevice, 1_000)
            }
            // Each time a track is reached it can count again (a repeat, a jump back); the one left is told.
            if (events.contains(Player.EVENT_MEDIA_ITEM_TRANSITION)) {
                leave("skipped")
                if (player.isPlaying) {
                    main.removeCallbacks(count)
                    main.post(count)
                }
            }
            // Nothing queued any more, or the end of the queue: the track left is told as stopped.
            if (events.contains(Player.EVENT_TIMELINE_CHANGED) && player.mediaItemCount == 0) leave("stopped")
            if (events.contains(Player.EVENT_PLAYBACK_STATE_CHANGED) && player.playbackState == Player.STATE_ENDED) leave("stopped")
            if (events.contains(Player.EVENT_IS_PLAYING_CHANGED)) {
                // Sounding here means controls here (decision 0062), whatever started it.
                if (player.isPlaying && prefs.target != Prefs.TARGET_SELF) pointAt(Prefs.TARGET_SELF, "its own player started")
                main.removeCallbacks(count)
                if (player.isPlaying) main.post(count)
                // A sleep timer that ran out while the music was paused only clears (its wait may not have come yet:
                // the device sleeps while nothing plays).
                val at = Sleep.get(this@PlayerService)?.atMs
                if (player.isPlaying && at != null && at <= System.currentTimeMillis()) Sleep.clear(this@PlayerService)
            }
            // Nothing queued any more: no timer.
            if (events.contains(Player.EVENT_TIMELINE_CHANGED) && player.mediaItemCount == 0 && Sleep.get(this@PlayerService) != null) {
                Sleep.clear(this@PlayerService)
            }
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            // The track the sleep timer waited for ended: the player paused itself, and the timer is done.
            if (reason == Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) Sleep.clear(this@PlayerService)
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_ENDED && Sleep.get(this@PlayerService)?.endOfTrack == true) Sleep.clear(this@PlayerService)
        }

        override fun onPlayerError(error: PlaybackException) {
            val cause = error.cause
            val status = (cause as? HttpDataSource.InvalidResponseCodeException)?.responseCode
            // Cratarr still out of reach after a minute of asking (Patience): the same track again in a while, where it
            // was. Refused (sign in again): every track would be, so it waits for the sign-in (Net tells the owner).
            // Otherwise, as a player does: what cannot play (gone, or out of reach offline) is skipped, unless it was
            // the last.
            val action = when {
                // The device's music, no longer allowed: every track would fail.
                player.currentMediaItem?.let(DeviceMusic::owns) == true && !DeviceMusic.granted(this@PlayerService) -> "stop"
                Patience.refused(cause) -> "wait for sign-in"
                Patience.waitFor(cause) -> "again"
                player.hasNextMediaItem() -> "skip"
                else -> "stop"
            }
            LinkLog.add(
                this@PlayerService, "warn", "playon", "track failed",
                "track" to player.currentMediaItem?.mediaId, "error" to error.errorCodeName, "status" to status, "action" to action,
            )
            when (action) {
                "again" -> main.postDelayed({ if (player.playerError != null) player.prepare() }, AGAIN_MS)
                "skip" -> {
                    player.seekToNextMediaItem()
                    player.prepare()
                    player.play()
                }
            }
        }
    }

    override fun onDestroy() {
        LinkLog.add(this, "info", "playon", "service stopped")
        main.removeCallbacksAndMessages(null)
        saveDevice.run()
        Prefs.shared(this).unregisterOnSharedPreferenceChangeListener(prefsChanged)
        sync.release()
        link?.stop()
        link = null
        session?.release()
        snippets.release()
        startStop.release()
        player.release()
        remote.release()
        session = null
        super.onDestroy()
    }

    /** Tracks arrive by their id only; the file and the cover come from the server. The app's own page may switch the
     *  mode ([COMMAND_MODE], `mode`). */
    /** A media key down and not let go yet (its hold waiting [HOLD_MS]), and whether it has become a hold. */
    private var keyDown: Runnable? = null
    private var keyHeld = false

    private inner class Callback : MediaSession.Callback {
        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
            if (controller.packageName != packageName) return super.onConnect(session, controller)
            // Media3's own defaults for a controller of the app's (every player command), and the mode. Not built on
            // super's answer: that is a stand-in with no commands at all, which Media3 replaces only when it is returned
            // as it is (copied, the page's player could do nothing: 69, fixed in 69.1).
            val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon()
                .add(SessionCommand(COMMAND_MODE, Bundle.EMPTY))
                .add(SessionCommand(COMMAND_TARGET, Bundle.EMPTY))
                .add(SessionCommand(COMMAND_RUN, Bundle.EMPTY))
                .add(SessionCommand(COMMAND_SNIPPET, Bundle.EMPTY))
                .build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session, controller).setAvailableSessionCommands(commands).build()
        }

        /**
         * A media key held (decision 0074; the owner, 2026-10-27): rewind and fast forward held, and previous and next held
         * past [HOLD_MS] (a headset's, a car's, Bluetooth's), wind while held and brake on letting go; a short press of
         * previous or next stays a press. Other keys as ever.
         */
        override fun onMediaButtonEvent(session: MediaSession, controllerInfo: MediaSession.ControllerInfo, intent: Intent): Boolean {
            @Suppress("DEPRECATION")
            val e = intent.getParcelableExtra<KeyEvent>(Intent.EXTRA_KEY_EVENT) ?: return false
            val code = e.keyCode
            val wind = code == KeyEvent.KEYCODE_MEDIA_REWIND || code == KeyEvent.KEYCODE_MEDIA_FAST_FORWARD
            if (!wind && code != KeyEvent.KEYCODE_MEDIA_PREVIOUS && code != KeyEvent.KEYCODE_MEDIA_NEXT) return false
            val back = code == KeyEvent.KEYCODE_MEDIA_REWIND || code == KeyEvent.KEYCODE_MEDIA_PREVIOUS
            val press = { if (back) sessionPlayer.seekToPrevious() else sessionPlayer.seekToNextMediaItem() }
            val holdNow = { keyHeld = true; startStop.windStart(back) { press() } }
            when (e.action) {
                KeyEvent.ACTION_DOWN -> when {
                    e.repeatCount == 0 && wind -> holdNow()
                    e.repeatCount == 0 -> {
                        keyDown = Runnable { holdNow() }.also { main.postDelayed(it, HOLD_MS) }
                    }
                    !keyHeld -> {
                        keyDown?.let(main::removeCallbacks)
                        keyDown = null
                        holdNow()
                    }
                }
                KeyEvent.ACTION_UP -> {
                    keyDown?.let(main::removeCallbacks)
                    keyDown = null
                    if (keyHeld) {
                        keyHeld = false
                        startStop.windStop()
                    } else if (!wind) {
                        press()
                    }
                }
            }
            return true
        }

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            when (customCommand.customAction) {
                COMMAND_MODE -> switchMode(args.getString("mode").orEmpty(), "page")
                COMMAND_TARGET -> pointAt(args.getString("to").orEmpty(), "the page asked")
                // The page's player commands, as Play On's (the page's own way where the two differ).
                COMMAND_RUN -> runFromPage(args.getString("command"))
                COMMAND_SNIPPET -> snippets.handle(args.getString("message"))
                else -> return super.onCustomCommand(session, controller, customCommand, args)
            }
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> = Futures.immediateFuture(mediaItems.map(::located).toMutableList())
    }

    /** A command from the page's own player (PagePlayer, [COMMAND_RUN]): Cratarr's shape, run as Play On's are. */
    private fun runFromPage(json: String?) {
        val m = try {
            JSONObject(json.orEmpty())
        } catch (e: JSONException) {
            Log.w(TAG, "a page command that cannot be read: $json")
            return
        }
        if (!commands.run(m, PlayerCommands.From.PAGE)) Log.w(TAG, "a page command this app does not know: $json")
    }

    /** A track's item with its file (a download's, else `/api/file/{source}/{id}`) and its cover from the server; the
     *  device's own from Android. */
    fun located(item: MediaItem): MediaItem {
        DeviceMusic.located(item)?.let { return it }
        val server = prefs.serverUrl?.trimEnd('/')
        val found = Downloads.locate(this, item, server)
        server ?: return found
        return found.buildUpon()
            .setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(Uri.parse("$server/api/cover/${item.mediaId}")).build())
            .build()
    }

    /** The playing track's wind stream (decision 0074): Cratarr's, from [fromMs], to [toMs] or the edge, condensed to
     *  [fitMs] or open-ended, back or forward, as this device's winding is set; none for a track of the device's own or a
     *  Bandcamp preview, which Cratarr keeps no file of. */
    private fun windUrl(fromMs: Long, toMs: Long?, fitMs: Long?, back: Boolean, through: String?, speed: Float, gainDb: Float, rise: Boolean): String? {
        val item = player.currentMediaItem ?: return null
        val server = prefs.serverUrl?.trimEnd('/') ?: return null
        if (DeviceMusic.located(item) != null || Previews.isPreview(item.mediaId)) return null
        val to = toMs?.let { "&to_ms=$it" } ?: ""
        val w = prefs.holdWind(back)
        val fit = fitMs?.let { "&fit_ms=$it&attack_ms=${w.attackMs}&release_ms=${w.releaseMs}&piece_ms=${w.pieceMs}" } ?: ""
        val on = through?.let { "&through=" + Uri.encode(it, ",/:") } ?: ""
        // A hold speeds up as it goes (0.9.12): doubling as the tape is set, up to its top.
        // Flying on past what is heard (0.9.14): heard at most at its top, on to the flight's in pieces.
        val accel = if (rise) "&accel_s=${w.doublingS}&max=${w.top}&flight_max=${Prefs.FLIGHT_TOP}&piece_ms=${w.pieceMs}" else ""
        return "$server/api/stream/${item.mediaId}/wind?from_ms=$fromMs&dir=${if (back) "back" else "forward"}&speed=$speed" +
            "&gain_db=$gainDb$to$fit$on$accel"
    }

    /**
     * A let-go's brake stream (0.9.14-3, the server's `wind::Brake`): the queue's track [index] from [fromMs], back or
     * forward, on through [through] to its stand, the plan from [v0] × to [to] × over [ms], the stream from [atMs] into
     * it; heard at most at the hold's top, in its pieces above it, as loud as the hold.
     */
    private fun brakeUrl(index: Int, fromMs: Long, back: Boolean, through: String?, v0: Double, to: Double, ms: Long, atMs: Long): String? {
        if (index !in 0 until player.mediaItemCount) return null
        val item = player.getMediaItemAt(index)
        val server = prefs.serverUrl?.trimEnd('/') ?: return null
        if (DeviceMusic.located(item) != null || Previews.isPreview(item.mediaId)) return null
        val w = prefs.holdWind(back)
        val on = through?.let { "&through=" + Uri.encode(it, ",/:") } ?: ""
        return "$server/api/stream/${item.mediaId}/wind?from_ms=$fromMs&dir=${if (back) "back" else "forward"}&speed=${w.speed}" +
            "&gain_db=${w.gainDb}&brake_v0=$v0&brake_to=$to&brake_ms=$ms&brake_at_ms=$atMs&max=${w.top}&piece_ms=${w.pieceMs}$on"
    }

    /** The queue's tracks in a hold's way (decision 0074, through the queue): back the ones before this, forward the ones
     *  after, nearest first, at most [WIND_THROUGH], as far as Cratarr has their files and knows their length (as the
     *  server stops). */
    private fun windWay(back: Boolean): List<Pair<String, Long>> {
        val i = player.currentMediaItemIndex
        val places = if (back) (i - 1 downTo 0) else (i + 1 until player.mediaItemCount)
        val out = mutableListOf<Pair<String, Long>>()
        for (j in places) {
            if (out.size >= WIND_THROUGH) break
            val item = player.getMediaItemAt(j)
            if (DeviceMusic.located(item) != null || Previews.isPreview(item.mediaId)) break
            val ms = trackOf(item)?.optDouble("duration_secs", 0.0)?.takeIf { it > 0 }?.let { (it * 1000).toLong() } ?: break
            out += item.mediaId to ms
        }
        return out
    }

    /** Local mode's shuffle, once: what is to come after the playing track, mixed. */
    private fun shuffleUpcoming(p: Player) {
        val from = p.currentMediaItemIndex + 1
        if (p.mediaItemCount - from < 2) return
        val upcoming = (from until p.mediaItemCount).map(p::getMediaItemAt).shuffled()
        p.replaceMediaItems(from, p.mediaItemCount, upcoming)
    }

    companion object {
        private const val TAG = "CratarrPlayer"
        const val RESTART_AFTER_MS = 5_000L
        /** A track Cratarr could not give for a minute: asked for again after this long. */
        private const val AGAIN_MS = 15_000L
        /** How often the playing track is looked at for counting. */
        private const val COUNT_EVERY_MS = 5_000L
        /** Playing on this far past a track's length is written down ([Course]), looked at this often. */
        private const val PAST_END_MS = 1_000L
        private const val PAST_CHECK_MS = 2_000L
        /** The most tracks a hold winds on into past its own: the server's (0.9.14-2; 8 before, a hold back stopped at
         *  the eighth track's start). */
        private const val WIND_THROUGH = 64
        private const val READY_CHANNEL = "ready"
        private const val GROUP_NOTIFICATION = 8
        private const val ACTION_READY = "cratarr.READY"
        /** The page's mode switch (decision 0052): `mode`, cratarr or device. */
        const val COMMAND_MODE = "cratarr.MODE"
        /** The page asks where the controls point (decision 0062): `to`, self, player:<name> or server. */
        const val COMMAND_TARGET = "cratarr.TARGET"
        /** The page's player commands (PagePlayer): `command`, one of Cratarr's player commands as JSON ([PlayerCommands]). */
        const val COMMAND_RUN = "cratarr.RUN"
        /** The page's track snippets (0.12, [Snippets]): `message`, the page's `{"type": "snippet", …}` as JSON. */
        const val COMMAND_SNIPPET = "cratarr.SNIPPET"
        /** A previous or next key held this long is a hold: it winds. */
        private const val HOLD_MS = 400L

        /** Started, and ready (or let go) as Stay ready says. Called while the app is in front, where both are allowed. */
        fun ready(context: Context) {
            try {
                context.startService(Intent(context, PlayerService::class.java).setAction(ACTION_READY))
            } catch (e: IllegalStateException) {
                Log.w(TAG, "cannot start: ${e.message}")
            }
            Ready.ensure(context, "app opened")
        }
    }
}
