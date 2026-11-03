/*
 * Whose this device's things are (decision 0101, points 13–15): each Cratarr account keeps its own downloads and kept
 * items, its queues and mode, its other servers (decision 0098) and its scrobbling sign-ins (decision 0091) here,
 * hidden while another account is signed in or nobody is, and shown again at its next sign-in. A file two accounts
 * downloaded is stored once (Kept's holders).
 * The key: `@name` for an account, `@` with signing in off (one user), [NONE] for the device's no-account side: signed
 * out, Local still plays, and the servers added then are its own.
 * Learnt from Cratarr's `/api/me` (Plays) and from the page; signed out at the page's Sign out, and when Cratarr says no
 * one is signed in. Those listening ([changed]) put the one left's things by and take up the one come.
 */
package app.cratarr

import android.content.Context
import android.content.SharedPreferences
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
    fun account(context: Context): String? = current(context).takeIf { it != NONE }?.removePrefix("@")

    /** A device setting's name for the one whose it is now (`scrobble_queue@name`). */
    fun keyed(context: Context, name: String): String = name + current(context)

    /** `account` is signed in (null: nobody is). */
    fun set(context: Context, account: String?) {
        val app = context.applicationContext
        val to = keyOf(account)
        val from = synchronized(this) {
            val was = current(app)
            if (was == to) return
            Prefs.shared(app).edit().putString(KEY, to).commit()
            was
        }
        for (f in changed) f(from, to)
    }
}
