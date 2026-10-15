package app.cratarr

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import androidx.appcompat.app.AppCompatActivity
import androidx.preference.EditTextPreference
import androidx.preference.Preference
import androidx.preference.PreferenceFragmentCompat

/** The server's address, this player's name, Stay ready, and battery optimisation. */
class SettingsActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        if (savedInstanceState == null) {
            supportFragmentManager.beginTransaction().replace(android.R.id.content, Screen()).commit()
        }
    }

    override fun onSupportNavigateUp(): Boolean {
        finish()
        return true
    }

    class Screen : PreferenceFragmentCompat() {
        override fun onCreatePreferences(savedInstanceState: Bundle?, rootKey: String?) {
            setPreferencesFromResource(R.xml.preferences, rootKey)
            findPreference<EditTextPreference>(Prefs.SERVER_URL)?.setOnPreferenceChangeListener { pref, value ->
                // Kept as it will be used: with its scheme, without a trailing slash.
                val typed = value.toString()
                if (typed.isNotBlank()) (pref as EditTextPreference).text = Prefs.normalise(typed)
                false
            }
            findPreference<EditTextPreference>(Prefs.PLAYER_NAME)?.summaryProvider =
                Preference.SummaryProvider<EditTextPreference> { it.text?.takeIf(String::isNotBlank) ?: Build.MODEL }
            findPreference<Preference>(Prefs.BATTERY)?.setOnPreferenceClickListener {
                askToRunInBackground()
                true
            }
        }

        override fun onResume() {
            super.onResume()
            val power = requireContext().getSystemService(PowerManager::class.java)
            findPreference<Preference>(Prefs.BATTERY)?.isEnabled = !power.isIgnoringBatteryOptimizations(requireContext().packageName)
        }

        private fun askToRunInBackground() {
            val ask = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${requireContext().packageName}"))
            try {
                startActivity(ask)
            } catch (e: ActivityNotFoundException) {
                startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
            }
        }
    }
}
