package app.cratarr

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.os.Build
import androidx.core.content.ContextCompat

/**
 * Where the device roughly is, for the moment's weather (decision 0101, point 18; the page's lib/momentHere), and only for that:
 * asked by the page with its switch on, as a WebView gives a page on a plain address no place of its own. Android's coarse
 * location, of which only the first decimal ever leaves here.
 */
object CoarseLocation {
    const val PERMISSION = Manifest.permission.ACCESS_COARSE_LOCATION

    /** A place known this recently needs no new fix: a weather model's square is some 10 km. */
    private const val RECENT_MS = 60 * 60_000L

    fun granted(context: Context) = ContextCompat.checkSelfPermission(context, PERMISSION) == PackageManager.PERMISSION_GRANTED

    /** Degrees as far as they go out. */
    fun coarse(degrees: Double): Double = Math.round(degrees * 10) / 10.0

    /** The device's place, on the main thread; null where Android knows of none. */
    fun now(context: Context, done: (Location?) -> Unit) {
        val locations = context.getSystemService(LocationManager::class.java)
        if (locations == null || !granted(context)) return done(null)
        val known = try {
            locations.getProviders(true).mapNotNull(locations::getLastKnownLocation).maxByOrNull { it.time }
        } catch (e: SecurityException) {
            null
        }
        val provider = locations.getProviders(true).firstOrNull { it == LocationManager.NETWORK_PROVIDER }
        if (provider == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.R || (known != null && System.currentTimeMillis() - known.time < RECENT_MS)) return done(known)
        try {
            locations.getCurrentLocation(provider, null, ContextCompat.getMainExecutor(context)) { done(it ?: known) }
        } catch (e: SecurityException) {
            done(known)
        }
    }
}
