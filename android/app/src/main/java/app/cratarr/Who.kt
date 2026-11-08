/*
 * Whose this device's things are (decision 0101, points 13–15): each Cratarr account keeps its own downloads and kept
 * items, its queues and mode, its other servers (decision 0098) and its scrobbling sign-ins (decision 0091) here,
 * hidden while another account is signed in or nobody is, and shown again at its next sign-in. A file two accounts
 * downloaded is stored once (Kept's holders).
 * The key: `@name` for an account, `@` with signing in off (one user), [NONE] for the device's no-account side: signed
 * out, Local still plays, and the servers added then are its own.
 * Learnt from Cratarr's `/api/me` (Plays) and from the page; signed out at the page's Sign out, when Cratarr says no one
 * is signed in, and at Cratarr's sign-in page (whoever signs in there is not known yet). Those listening ([changed]) put
 * the one left's things by and take up the one come; what is only a setting goes over here ([Prefs.handOver]).
 */
package app.cratarr

import android.content.Context
import android.content.SharedPreferences
import org.json.JSONObject
import java.util.concurrent.CopyOnWriteArraySet

object Who {
    /** The device's no-account side. */
    const val NONE = "-"
    const val KEY = "who"

    /** Told, on the thread that learnt it, when another one's things are this device's: the key left, the key come. */
    val changed = CopyOnWriteArraySet<(from: String, to: String) -> Unit>()

    /** An account's key: its name (`""` with signing in off), or null for nobody. */
    fun keyOf(account: String?): String = if (account == null) NONE else "@$account"

    /** Whose things show now. */
    fun current(context: Context): String = current(Prefs.shared(context))

    fun current(p: SharedPreferences): String = p.getString(KEY, NONE) ?: NONE

    /** The account signed in now: its name, `""` with signing in off; null for nobody. */
    fun account(context: Context): String? = accountOf(current(context))

    /** The account of a key: its name, `""` with signing in off; null for nobody. */
    fun accountOf(who: String): String? = who.takeIf { it != NONE }?.removePrefix("@")

    /** [who] in the page's own words (lib/account's `cratarr-who`, as JSON): the name, or null for nobody. */
    fun forPage(who: String): String = if (who == NONE) "null" else JSONObject.quote(who.removePrefix("@"))

    /** A device setting's name for the one whose it is now (`scrobble_queue@name`). */
    fun keyed(context: Context, name: String): String = name + current(context)

    /** `account` is signed in (null: nobody is). */
    fun set(context: Context, account: String?) {
        val app = context.applicationContext
        val to = keyOf(account)
        val from = synchronized(this) {
            val p = Prefs.shared(app)
            val was = current(p)
            if (was == to) return
            Prefs.handOver(p, was, to)
            p.edit().putString(KEY, to).commit()
            was
        }
        // Here, not a listener's: a play told behind a locked screen can bring this about with no activity open.
        Notices.forget(app)
        ExportSave.forget(app)
        Kept.sync(app)
        DeviceIndex.start(app)
        FreshStart.look(app)
        for (f in changed) f(from, to)
    }
}
