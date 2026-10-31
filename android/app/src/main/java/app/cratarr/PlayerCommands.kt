package app.cratarr

import androidx.media3.common.Player
import org.json.JSONArray
import org.json.JSONObject

/**
 * The one way into the app's own player for a command (the architecture audit, 2026-10-29: "one command path into the
 * player"): Play On's from Cratarr (PlayOnLink) and the page's from its own player (PagePlayer, through the session's
 * [PlayerService.COMMAND_RUN]) alike, in the shape Cratarr sends a player (its `Command`: `{"command": …}`, snake_case).
 * Where the two did differently before, each still does its own, by [From]; those places say so.
 */
class PlayerCommands(
    private val service: PlayerService,
    /** The player itself. */
    private val player: Player,
    /** The player as the session shows it: its play, pause, stop, seeks, next and previous go through Start and stop,
     *  as a controller's (the page's) reach it. */
    private val shown: Player,
    private val prefs: Prefs,
    private val startStop: StartStop,
    /** Play On's link, for its part in a start Android may refuse (it asked, it held back); null while there is none. */
    private val link: () -> PlayOnLink?,
) {
    /** Who sent a command: the page's own player in the app, or Cratarr (Play On). */
    enum class From { PAGE, PLAY_ON }

    /** Runs command [m]; false when it is not one of the player's (volume, group, announce… are Play On's own). */
    fun run(m: JSONObject, from: From): Boolean {
        val command = m.optString("command")
        val p = player
        val local = prefs.mode == Prefs.MODE_DEVICE
        // Local mode (decision 0052), from Cratarr: steered from outside, its queue not edited (Cratarr refuses such
        // commands; one from a moment before is let go). A play is Cratarr's music again; a stop pauses, so that queue
        // stays. The page's own player edits local mode's queue as ever: its tracks of the other mode are left out, and
        // Radio, Autoplay and skipping favourites do nothing (below).
        if (local && from == From.PLAY_ON) {
            when (command) {
                "play" -> service.switchMode(Prefs.MODE_CRATARR, "play from Cratarr")
                "pause", "resume", "toggle", "next", "previous", "sleep", "seek", "wind", "direction", "level" -> {}
                "stop" -> {
                    link()?.release()
                    p.pause()
                    followMusic(m)
                    return true
                }
                else -> {
                    LinkLog.add(service, "info", "playon", "command not for local mode", "command" to command)
                    return true
                }
            }
        }
        when (command) {
            "play" -> if (from == From.PAGE) playFromPage(m) else play(m)
            // Edits name entries (decision 0033); by place only when the entry is not found here. The page's by place
            // only, as ever (its `entry` and `after` are for a player it drives), the tracks of the mode it plays.
            "add" -> if (from == From.PAGE) {
                p.addMediaItems(m.optInt("at", p.mediaItemCount), mediaItems(only(m.optJSONArray("tracks"), prefs.mode)).map(service::located))
            } else {
                val items = mediaItems(m.optJSONArray("tracks") ?: JSONArray()).map(service::located)
                val after = m.optString("after", null)?.let { if (it.isEmpty()) 0 else placeOf(p, it).takeIf { a -> a >= 0 }?.plus(1) }
                val at = after ?: if (m.has("at")) m.optInt("at").coerceIn(0, p.mediaItemCount) else p.mediaItemCount
                p.addMediaItems(at, items)
                if (p.playbackState == Player.STATE_IDLE) p.prepare()
            }
            "move" -> if (from == From.PAGE) {
                p.moveMediaItem(m.optInt("from"), m.optInt("to"))
            } else {
                val was = placed(m, "entry", "from")
                val after = m.optString("after", null)?.let { if (it.isEmpty()) 0 else placeOf(p, it) }
                // The place it ends up at, counted with it taken out.
                val to = when {
                    after == null || after < 0 -> m.optInt("to", -1)
                    after == 0 && m.optString("after").isEmpty() -> 0
                    after > was -> after
                    else -> after + 1
                }
                if (was in 0 until p.mediaItemCount && to in 0 until p.mediaItemCount && was != to) p.moveMediaItem(was, to)
            }
            "remove" -> if (from == From.PAGE) {
                p.removeMediaItem(m.optInt("index"))
            } else {
                placed(m, "entry", "index").takeIf { it in 0 until p.mediaItemCount }?.let(p::removeMediaItem)
            }
            // Everything after the playing track replaced (a shuffle turned on or off); the playing track plays on.
            "upcoming" -> {
                if (m.has("shuffled")) prefs.shuffled = m.optBoolean("shuffled")
                val tracks = if (from == From.PAGE) only(m.optJSONArray("tracks"), prefs.mode) else m.optJSONArray("tracks") ?: JSONArray()
                val items = mediaItems(tracks).map(service::located)
                val after = (p.currentMediaItemIndex + 1).coerceAtMost(p.mediaItemCount)
                p.replaceMediaItems(after, p.mediaItemCount, items)
            }
            // The page's by place, through the session's player (its Start and stop seek), then its play; Cratarr's by
            // entry, forwards, at once, then Play On's play.
            "jump" -> if (from == From.PAGE) {
                shown.seekTo(m.optInt("index"), 0)
                resume(From.PAGE)
            } else {
                placed(m, "entry", "index").takeIf { it in 0 until p.mediaItemCount }?.let {
                    // The knob's way, the knob left as set (0.12.4: pure directions, the owner 2026-10-31): at L the
                    // track backwards from its end (`StartStop.jump`); before, forwards with the knob at R.
                    startStop.jump { p.seekTo(it, 0) }
                    resume(From.PLAY_ON)
                }
            }
            // Start and stop as the device says (0.9.6). Cratarr's lets go of a start Android held back.
            "pause" -> {
                if (from == From.PLAY_ON) link()?.release()
                startStop.pause { p.pause() }
            }
            "resume" -> resume(from)
            "toggle" -> if (from == From.PAGE) {
                // The page's, as the session shows the player (a ramp's way, not the track's); no `plain` here.
                if (shown.playWhenReady && shown.playbackState != Player.STATE_ENDED) shown.pause() else resume(From.PAGE)
            } else {
                // `plain`: a page driving this player while it switches devices: at once, as a stop (the owner, 2026-10-27).
                val plain = m.optBoolean("plain")
                if (p.playWhenReady && p.playbackState != Player.STATE_ENDED) {
                    if (plain) {
                        startStop.reset()
                        p.pause()
                    } else {
                        startStop.pause { p.pause() }
                    }
                } else {
                    resume(From.PLAY_ON, plain = plain)
                }
            }
            "stop" -> if (from == From.PAGE) {
                // The page's: wound down as its pause, the queue cleared at once.
                shown.stop()
                p.clearMediaItems()
            } else {
                // A move's hold: stopped at once, no wind-down (switching devices bypasses every play effect, the owner).
                link()?.release()
                startStop.reset()
                p.stop()
                p.clearMediaItems()
                followMusic(m)
            }
            // Next as the app's own: fast-forward to the end where winding is on, else a skip (0.9.6, Start and stop:
            // the change shows once it is made, after a ramp out). Previous: a rewind to the start where it winds, else
            // the restart or the track before; under repeat all, the first track's is the last one.
            "next" -> shown.seekToNextMediaItem()
            "previous" -> shown.seekToPrevious()
            "repeat" -> Repeat.set(service, p, m.optString("mode"), service::located)
            "sleep" -> Sleep.set(service, m, "at_ms", "end_of_track")
            // Cratarr's alone: not for the device's own music.
            "radio" -> if (!local) Radio.command(service, p, m)
            "autoplay" -> if (!local) Autoplay.command(service, p, m)
            "skip_favourites" -> if (!local) SkipFavourites.set(service, p, m.optBoolean("on"))
            // `wind`: wound there whatever this device says (a page's seek winding). Without a place: Cratarr's stays where
            // it is, the page's goes to the start.
            "seek" -> m.optLong("position_ms", if (from == From.PAGE) 0L else p.currentPosition).let { to ->
                startStop.seekTo(to, m.optBoolean("wind")) { p.seekTo(to) }
            }
            // Level the volume set from a page driving this player (0.9.12), eased as its own switch is.
            "level" -> {
                if (m.has("on")) prefs.levelling = m.optBoolean("on")
                m.optString("unknown").takeIf { it == Prefs.LEVEL_TYPICAL || it == Prefs.LEVEL_UNTOUCHED }?.let { prefs.levelUnknown = it }
            }
            // The tape deck's direction knob (0.9.12).
            "direction" -> startStop.setDirection(m.optString("dir") == "L")
            // Previous or Next held (decision 0074): wound while held; where it cannot wind, a press on letting go: the
            // page's as its Previous or Next (a rewind, a fast-forward where they wind), Cratarr's a skip.
            "wind" -> {
                val back = m.optString("dir") != "forward"
                if (m.optString("phase") == "stop") {
                    startStop.windStop()
                } else {
                    startStop.windStart(back) {
                        when {
                            from == From.PAGE && back -> shown.seekToPrevious()
                            from == From.PAGE -> shown.seekToNextMediaItem()
                            back -> startStop.skip { if (!Repeat.previous(service, p)) p.seekToPrevious() }
                            else -> startStop.skip { p.seekToNextMediaItem() }
                        }
                    }
                }
            }
            else -> return false
        }
        return true
    }

    /**
     * Plays: the page's through the session's player, wound up as Start and stop says (its prepare and an ended track's
     * restart as a controller's were); Cratarr's wound up too, or ([plain]) at once, told to Play On as asked ([again]:
     * once more, after Android refused it).
     */
    fun resume(from: From, again: Boolean = false, plain: Boolean = false) {
        val p = player
        if (from == From.PAGE) {
            if (p.playbackState == Player.STATE_IDLE) shown.prepare()
            if (p.playbackState == Player.STATE_ENDED) shown.seekTo(p.currentMediaItemIndex, 0)
            shown.play()
            return
        }
        if (p.playbackState == Player.STATE_IDLE) p.prepare()
        if (p.playbackState == Player.STATE_ENDED) p.seekTo(p.currentMediaItemIndex, 0)
        link()?.asked(again)
        if (plain) {
            startStop.reset()
            p.play()
        } else {
            startStop.play { p.play() }
        }
    }

    /** A play from Cratarr (a move's carry-on, a queue taken up): its queue named as Cratarr names it, started at once. */
    private fun play(m: JSONObject) {
        val p = player
        val items = mediaItems(m.optJSONArray("tracks") ?: JSONArray()).map(service::located)
        if (items.isEmpty()) return
        p.setMediaItems(items, m.optInt("index", 0).coerceIn(0, items.size - 1), m.optLong("position_ms", 0))
        p.prepare()
        // A new play turns repeat one off; whether it comes shuffled counts for repeat all's next rounds; the radio
        // stays on, its seed the new queue (unless these are its own tracks).
        Repeat.newPlay(service, p)
        Autoplay.newPlay(service, m.optJSONArray("tracks"))
        prefs.shuffled = m.optBoolean("shuffle")
        // The queue as Cratarr names it (decision 0033, amended); a server from before names none: a new one.
        if (m.has("queue_id")) {
            prefs.queueId = m.optString("queue_id")
            prefs.queueName = m.optString("queue_name")
        } else {
            prefs.newQueue("")
        }
        // A queue given back after the app came back without one waits for a play.
        link()?.release()
        // Switching devices bypasses every play effect (the owner, 2026-10-27): no wind-up, no wind under way. A new
        // queue: forwards, the knob at R.
        startStop.forwards()
        if (m.optBoolean("paused")) p.pause() else {
            link()?.asked()
            startStop.reset()
            p.play()
        }
    }

    /**
     * A play the page started here (its `queue`): repeat one off, its shuffle, and in Cratarr's mode Autoplay's seed and
     * a new queue of its own named for the play (decision 0033, amended); only the tracks of the mode it plays; then
     * played as its play button would (Start and stop's wind-up), or loaded paused.
     */
    private fun playFromPage(m: JSONObject) {
        val p = player
        val mode = prefs.mode
        Repeat.newPlay(service, p)
        prefs.shuffled = m.optBoolean("shuffle")
        if (mode != Prefs.MODE_DEVICE) {
            Autoplay.newPlay(service, m.optJSONArray("tracks"))
            prefs.newQueue(m.optString("name"))
        }
        // A new queue starts forwards, the knob at R, any backwards play dropped (0.12.4: pure directions, the owner
        // 2026-10-31: the one moment the setting changes but by the user's turn; the page's the same).
        startStop.forwards()
        p.setMediaItems(mediaItems(only(m.optJSONArray("tracks"), mode)).map(service::located), m.optInt("index", 0), m.optLong("position_ms", 0))
        p.prepare()
        shown.playWhenReady = !m.optBoolean("paused")
    }

    /**
     * A handover's stop names where the music went (the owner, 2026-10-30: "the screen follows the music"): where this
     * device's controls pointed at its own player, they point there now, as if it was picked in Play On (decision
     * 0062), so the page shows that player's mini player and its controls drive it. Pointed elsewhere, they stay.
     */
    private fun followMusic(m: JSONObject) {
        val to = MovedTo.ofStop(m) ?: return
        if (prefs.target != Prefs.TARGET_SELF) return
        service.pointAt(to, "the music moved there")
    }

    /** The place of the entry a command names, else the place it gives. */
    private fun placed(m: JSONObject, entry: String, place: String): Int =
        m.optString(entry, null)?.let { placeOf(player, it) }?.takeIf { it >= 0 } ?: m.optInt(place, -1)

    private companion object {
        /** Only the tracks of [mode]: never both in one queue (decision 0052). */
        fun only(tracks: JSONArray?, mode: String): JSONArray {
            val out = JSONArray()
            for (i in 0 until (tracks?.length() ?: 0)) {
                val t = tracks?.optJSONObject(i) ?: continue
                if ((t.optString("source") == DeviceMusic.SOURCE) == (mode == Prefs.MODE_DEVICE)) out.put(t)
            }
            return out
        }
    }
}

/**
 * Where the music went at a handover (the owner, 2026-10-30: "the screen follows the music"): Play On's stop names the
 * player (`moved_to`), and so does the view of the player it left, a while (`moved_to`: its `player` and `at`).
 */
object MovedTo {
    /** The target ([Prefs.target]) a stop names; null for a plain stop. */
    fun ofStop(m: JSONObject): String? = if (m.isNull("moved_to")) null else target(m.optString("moved_to"))

    /** The target a player's view names, with when the music moved (epoch ms); null where it moved nowhere. */
    fun ofView(view: JSONObject?): Pair<String, Long>? {
        val m = view?.optJSONObject("moved_to") ?: return null
        return target(m.optString("player"))?.let { it to m.optLong("at") }
    }

    private fun target(name: String): String? = name.trim().takeIf { it.isNotEmpty() }?.let { "player:$it" }
}
