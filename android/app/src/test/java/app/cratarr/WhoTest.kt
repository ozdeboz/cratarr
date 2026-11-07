package app.cratarr

import android.content.SharedPreferences
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Whose the device's things are (decision 0101), and what is only a setting going over with them. */
class WhoTest {
    /** Settings in memory, as the app's settings file keeps them. */
    private class Memory : SharedPreferences {
        val values = mutableMapOf<String, Any?>()

        override fun getAll(): MutableMap<String, *> = values.toMutableMap()
        override fun getString(key: String?, defValue: String?): String? = values[key] as String? ?: defValue
        override fun getStringSet(key: String?, defValues: MutableSet<String>?): MutableSet<String>? = defValues
        override fun getInt(key: String?, defValue: Int): Int = values[key] as Int? ?: defValue
        override fun getLong(key: String?, defValue: Long): Long = values[key] as Long? ?: defValue
        override fun getFloat(key: String?, defValue: Float): Float = values[key] as Float? ?: defValue
        override fun getBoolean(key: String?, defValue: Boolean): Boolean = values[key] as Boolean? ?: defValue
        override fun contains(key: String?): Boolean = values.containsKey(key)
        override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) {}
        override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) {}

        override fun edit(): SharedPreferences.Editor = object : SharedPreferences.Editor {
            private val changes = mutableListOf<() -> Unit>()
            private fun change(f: () -> Unit): SharedPreferences.Editor {
                changes.add(f)
                return this
            }

            override fun putString(key: String, value: String?): SharedPreferences.Editor = change { values[key] = value }
            override fun putStringSet(key: String, value: MutableSet<String>?): SharedPreferences.Editor = change { values[key] = value }
            override fun putInt(key: String, value: Int): SharedPreferences.Editor = change { values[key] = value }
            override fun putLong(key: String, value: Long): SharedPreferences.Editor = change { values[key] = value }
            override fun putFloat(key: String, value: Float): SharedPreferences.Editor = change { values[key] = value }
            override fun putBoolean(key: String, value: Boolean): SharedPreferences.Editor = change { values[key] = value }
            override fun remove(key: String): SharedPreferences.Editor = change { values.remove(key) }
            override fun clear(): SharedPreferences.Editor = change { values.clear() }
            override fun commit(): Boolean {
                apply()
                return true
            }
            override fun apply() {
                changes.forEach { it() }
                changes.clear()
            }
        }
    }

    private val ann = "@ann"
    private val bo = "@bo"

    /** A device with ann's queue on it: Autoplay seeded with her tracks, Radio on, her controls on another player. */
    private fun hers() = Memory().apply {
        values[Prefs.QUEUE_ID] = "her-queue"
        values[Prefs.QUEUE_NAME] = "Her record"
        values[Prefs.RADIO_ON] = true
        values[Prefs.RADIO_BY] = "era"
        values[Prefs.RADIO_COUNT] = 3
        values[Prefs.SKIP_FAVOURITES] = true
        values[Prefs.AUTOPLAY] = """{"mode":"similar","seed":["tr-hers"],"within":{"kind":"crate","id":"cr-hers"}}"""
        values[Prefs.TARGET] = "player:Her room"
        values[Prefs.TARGET_WHY] = "she chose it"
    }

    @Test
    fun an_account_the_one_user_and_nobody_are_named_as_the_page_names_them() {
        assertEquals("@ann", Who.keyOf("ann"))
        assertEquals("@", Who.keyOf(""))
        assertEquals("-", Who.keyOf(null))
    }

    @Test
    fun the_page_is_told_whose_it_is_in_its_own_words() {
        assertEquals("\"ann\"", Who.forPage("@ann"))
        assertEquals("sign-in off", "\"\"", Who.forPage("@"))
        assertEquals("null", Who.forPage(Who.NONE))
        assertEquals("a name is told whole, whatever is in it", "a\"b", JSONObject("""{"n":${Who.forPage("@a\"b")}}""").getString("n"))
    }

    @Test
    fun the_next_one_gets_nothing_a_queue_of_hers_carried() {
        val p = hers()
        Prefs.handOver(p, ann, bo)
        for (k in listOf(Prefs.QUEUE_ID, Prefs.QUEUE_NAME, Prefs.RADIO_ON, Prefs.RADIO_BY, Prefs.RADIO_COUNT, Prefs.SKIP_FAVOURITES, Prefs.AUTOPLAY, Prefs.TARGET_WHY)) {
            assertFalse(k, p.values.containsKey(k))
        }
    }

    @Test
    fun nobody_has_her_seed_and_no_player_to_point_at() {
        val p = hers()
        Prefs.handOver(p, ann, Who.NONE)
        assertNull(p.getString(Prefs.AUTOPLAY, null))
        assertEquals(Prefs.TARGET_SELF, p.getString(Prefs.TARGET, null))
    }

    @Test
    fun the_controls_are_said_to_be_here_not_left_unsaid_or_the_page_would_seed_its_last_choice() {
        val p = hers()
        Prefs.handOver(p, ann, bo)
        assertEquals(Prefs.TARGET_SELF, p.values[Prefs.TARGET])
    }

    @Test
    fun all_of_it_is_hers_again_at_her_return() {
        val p = hers()
        val was = p.values.toMap()
        Prefs.handOver(p, ann, bo)
        Prefs.handOver(p, bo, ann)
        for ((k, v) in was) assertEquals(k, v, p.values[k])
    }

    @Test
    fun what_the_next_one_changed_is_its_own_and_goes_by_with_it() {
        val p = hers()
        Prefs.handOver(p, ann, bo)
        p.values[Prefs.QUEUE_ID] = "his-queue"
        p.values[Prefs.RADIO_COUNT] = 1
        Prefs.handOver(p, bo, ann)
        assertEquals("her-queue", p.values[Prefs.QUEUE_ID])
        assertEquals(3, p.values[Prefs.RADIO_COUNT])
        Prefs.handOver(p, ann, bo)
        assertEquals("his-queue", p.values[Prefs.QUEUE_ID])
        assertEquals(1, p.values[Prefs.RADIO_COUNT])
        assertFalse("and none of hers with it", p.values.containsKey(Prefs.AUTOPLAY))
    }

    @Test
    fun what_is_the_devices_stays_as_it_is() {
        val p = hers()
        p.values[Prefs.PLAYER_NAME] = "The tablet"
        p.values[Prefs.SYNC_BUDGET_GB] = 20L
        p.values[Prefs.EXITS_SEEN] = 7L
        Prefs.handOver(p, ann, bo)
        assertEquals("The tablet", p.values[Prefs.PLAYER_NAME])
        assertEquals(20L, p.values[Prefs.SYNC_BUDGET_GB])
        assertEquals("what Android told of a run is the device's (DeviceWatch)", 7L, p.values[Prefs.EXITS_SEEN])
    }

    // What runs only on a device, read from its source: says a line is there, not that it works.

    private fun source(name: String) = File("src/main/java/app/cratarr/$name").readText().replace("\r\n", "\n")

    @Test
    fun what_a_queue_carries_goes_over_with_whose_things_they_are_the_players_service_running_or_not() {
        val set = source("Who.kt").substringAfter("fun set(")
        val over = set.indexOf("Prefs.handOver(p, was, to)")
        assertTrue("not handed over", over >= 0)
        assertTrue("before the listeners hear of it", over < set.indexOf("for (f in changed)"))
    }

    @Test
    fun what_goes_out_to_a_service_comes_back_to_the_account_it_was_read_for() {
        val scrobblers = source("Scrobblers.kt")
        for (f in listOf("private fun sendHere(", "private fun handOver(", "fun lastFmBack(")) {
            val body = scrobblers.substringAfter(f).substringBefore("\n    }\n")
            assertTrue(f, "val who = Who.current(context)" in body)
            assertFalse(f, "Who.keyed(" in body)
            assertFalse("a sign-in sealed as whoever has signed in by then: $f", "keep(context, k)" in body)
        }
    }

    @Test
    fun a_download_being_saved_goes_by_its_name_only_while_whoever_asked_is_signed_in() {
        assertEquals("Her road trip.zip", ExportPaths.titleOf("Her road trip.zip", ann, ann))
        assertEquals("gone by the time it ends", null, ExportPaths.titleOf("Her road trip.zip", ann, Who.NONE))
        assertEquals(null, ExportPaths.titleOf("Her road trip.zip", ann, bo))
        assertEquals("nothing of its own to say", null, ExportPaths.titleOf(" ", ann, ann))
        assertEquals("the service's note as it starts, no download named", null, ExportPaths.titleOf(null, null, ann))
    }

    @Test
    fun what_the_tray_says_of_the_one_left_goes_with_them_with_no_activity_open() {
        val set = source("Who.kt").substringAfter("fun set(").substringBefore("for (f in changed)")
        assertTrue("Notices.forget(app)" in set)
        assertTrue("ExportSave.forget(app)" in set)
        assertTrue(".cancel(NOTIFICATION)" in source("Notices.kt").substringAfter("fun forget("))
        assertTrue(".cancel(DONE_NOTIFICATION)" in source("ExportSave.kt").substringAfter("fun forget("))
        assertTrue("every note of a download", "ExportPaths.titleOf(job?.name, job?.who, Who.current(context))" in source("ExportSave.kt"))
        assertTrue("whose it is, from the start", "ExportSave.Job(doc, token, message.optString(\"name\"), Who.current(this))" in source("MainActivity.kt"))
    }

    @Test
    fun a_sleep_timer_goes_with_whoever_set_it() {
        val p = Memory()
        p.values[Prefs.SLEEP] = "123,false"
        Prefs.handOver(p, ann, bo)
        assertNull(p.values[Prefs.SLEEP])
    }

    @Test
    fun what_is_on_its_way_across_a_change_is_not_the_next_ones() {
        val fill = source("Radio.kt").substringAfter("val who = Who.current(app)")
        assertTrue("an Autoplay pick looks before it lands", fill.indexOf("if (Who.current(app) != who) return@post") in 0 until fill.indexOf("player.addMediaItems"))
        assertTrue("the track downloading is named to one who holds it", "Download.STATE_DOWNLOADING && KeptHolds.goesOn(holders[it.request.id], who)" in source("Kept.kt"))
    }

    @Test
    fun the_device_music_played_here_is_each_accounts_own() {
        val plays = source("Plays.kt")
        assertEquals(3, Regex("Who\\.keyed\\(context, PLAYED_HERE\\)").findAll(plays).count())
        assertEquals("and no other use of it, the name's own line aside", 4, Regex("\\bPLAYED_HERE\\b").findAll(plays).count())
        assertTrue("told at a change", "Plays.playedHere(this)" in source("MainActivity.kt").substringAfter("private val whoChanged").substringBefore("override fun onDestroy"))
    }

    @Test
    fun a_play_is_the_ones_it_was_heard_under_however_late_it_is_told() {
        assertEquals("ann", Who.accountOf(ann))
        assertEquals("sign-in off", "", Who.accountOf(Who.keyOf("")))
        assertNull(Who.accountOf(Who.NONE))
        val service = source("PlayerService.kt")
        assertTrue("Hearing(it, System.currentTimeMillis(), queueWho)" in service)
        assertEquals("each place it is told from", 3, Regex("Plays\\.heard\\(this@PlayerService, h\\.who,").findAll(service).count())
        val heard = source("Plays.kt").substringAfter("fun heard(").substringBefore("\n    }\n")
        assertTrue("val account = Who.accountOf(who)" in heard)
        assertTrue("the queue named is the one signed in's by then", "who == Who.current(context)" in heard)
    }

    @Test
    fun the_player_takes_up_whoever_it_is_by_then() {
        assertTrue("main.post { whoCame(Who.current(this)) }" in source("PlayerService.kt"))
    }

    @Test
    fun a_kept_word_that_cannot_be_read_is_nothing_of_anyones() {
        val p = hers()
        p.values["carried$bo"] = "cut off"
        Prefs.handOver(p, ann, bo)
        assertFalse(p.values.containsKey(Prefs.QUEUE_ID))
        assertEquals(Prefs.TARGET_SELF, p.values[Prefs.TARGET])
    }
}
