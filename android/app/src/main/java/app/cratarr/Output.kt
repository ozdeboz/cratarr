/*
 * Where the music sounds (decision 0033, amended 2026-10-19): the device Android routes media to now, told to Cratarr
 * with each report, so a queue remembers where it was heard ("the car queue") and one started in the car can take the
 * car's name. Read from Android's own routing (no permission asked); a change (Bluetooth on or off, headphones in or
 * out) is reported at once. Automatic can follow the owner's order of every output seen here ([preferred]).
 */
package app.cratarr

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

object Output {
    /** The output media plays through now, as `{type, name}` (`name` null for the speaker or where unknown); null when
     *  Android says none. */
    fun now(context: Context): JSONObject? {
        val audio = context.getSystemService(AudioManager::class.java) ?: return null
        val device = if (Build.VERSION.SDK_INT >= 33) {
            val music = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build()
            audio.getAudioDevicesForAttributes(music).firstOrNull()
        } else {
            // Before Android 13: the most likely of the outputs there are (a Bluetooth or wired one over the speaker).
            audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).maxByOrNull { rank(typeOf(it.type)) }
        } ?: return null
        val type = typeOf(device.type)
        val name = device.productName?.toString()?.trim()?.takeIf { it.isNotEmpty() && type != "speaker" }
        return JSONObject().put("type", type).put("name", name ?: JSONObject.NULL)
    }

    /** Every output there is now, as `{id, type, name}` (the id `type:name`, or `type` alone without a name), each once:
     *  the device sheet's choice (decision 0033, amended 2026-10-19). */
    fun all(context: Context): List<JSONObject> {
        val audio = context.getSystemService(AudioManager::class.java) ?: return emptyList()
        return audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
            .filter(::choosable)
            .map { d -> Triple(idOf(d), typeOf(d.type), nameOf(d)) }
            .distinctBy { it.first }
            .sortedByDescending { rank(it.second) }
            .map { (id, type, name) -> JSONObject().put("id", id).put("type", type).put("name", name ?: JSONObject.NULL) }
    }

    /** The output an id names, among those there are now; null when it is gone. */
    fun find(context: Context, id: String): AudioDeviceInfo? {
        val audio = context.getSystemService(AudioManager::class.java) ?: return null
        return audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).filter(::choosable).firstOrNull { idOf(it) == id }
    }

    /** Every output seen here, there now or not, in the owner's order: new ones go at the end (and are kept). */
    fun known(context: Context): List<JSONObject> {
        val prefs = Prefs(context)
        val kept = read(prefs.outputKnown)
        val ids = kept.map { it.optString("id") }.toSet()
        val new = all(context).filter { it.getString("id") !in ids }
        if (new.isEmpty()) return kept
        val out = kept + new
        prefs.outputKnown = JSONArray(out).toString()
        return out
    }

    /** The owner's order after a drag: the known outputs by [ids]; any not named keep their place after those. */
    fun order(context: Context, ids: List<String>) {
        val known = known(context)
        val byId = known.associateBy { it.optString("id") }
        val ordered = ids.distinct().mapNotNull(byId::get)
        Prefs(context).outputKnown = JSONArray(ordered + known.filter { it !in ordered }).toString()
    }

    /** An output not there now, forgotten (one that is there stays: it would come back at once). */
    fun forget(context: Context, id: String) {
        if (all(context).any { it.getString("id") == id }) return
        Prefs(context).outputKnown = JSONArray(known(context).filter { it.optString("id") != id }).toString()
    }

    /**
     * Where the player sends its sound: the output chosen in the sheet while there is one; else, with the owner's order
     * on, the first output of it that is there now; else none (Android chooses).
     */
    fun preferred(context: Context): AudioDeviceInfo? {
        val prefs = Prefs(context)
        prefs.outputChoice.takeIf { it.isNotEmpty() }?.let { return find(context, it) }
        if (!prefs.outputOrdered) return null
        val there = all(context).map { it.getString("id") }.toSet()
        val first = known(context).map { it.optString("id") }.firstOrNull { it in there } ?: return null
        return find(context, first)
    }

    private fun read(json: String): List<JSONObject> = try {
        val a = JSONArray(json)
        (0 until a.length()).mapNotNull { a.optJSONObject(it) }.filter { it.optString("id").isNotEmpty() }
    } catch (e: JSONException) {
        emptyList()
    }

    /** An output music can be sent to: not a call's (a Bluetooth device's call channel, the earpiece), nor the speaker's
     *  "safe" twin, nor something unknown. */
    private fun choosable(d: AudioDeviceInfo): Boolean = typeOf(d.type) !in setOf("other", "remote") &&
        d.type !in setOf(AudioDeviceInfo.TYPE_BUILTIN_EARPIECE, AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE, AudioDeviceInfo.TYPE_BLUETOOTH_SCO)

    /** Which outputs there are, as one text: a choice lasts until it changes. */
    fun set(context: Context): String = all(context).joinToString("|") { it.getString("id") }

    private fun nameOf(d: AudioDeviceInfo): String? =
        d.productName?.toString()?.trim()?.takeIf { it.isNotEmpty() && typeOf(d.type) != "speaker" }

    private fun idOf(d: AudioDeviceInfo): String = typeOf(d.type).let { t -> nameOf(d)?.let { "$t:$it" } ?: t }

    private fun typeOf(t: Int): String = when (t) {
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET,
        AudioDeviceInfo.TYPE_BLE_SPEAKER, AudioDeviceInfo.TYPE_BLE_BROADCAST, AudioDeviceInfo.TYPE_HEARING_AID -> "bluetooth"
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, AudioDeviceInfo.TYPE_BUILTIN_EARPIECE, AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE -> "speaker"
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES, AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_LINE_ANALOG,
        AudioDeviceInfo.TYPE_LINE_DIGITAL, AudioDeviceInfo.TYPE_AUX_LINE -> "wired"
        AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_ACCESSORY, AudioDeviceInfo.TYPE_DOCK -> "usb"
        AudioDeviceInfo.TYPE_HDMI, AudioDeviceInfo.TYPE_HDMI_ARC, AudioDeviceInfo.TYPE_HDMI_EARC -> "hdmi"
        AudioDeviceInfo.TYPE_REMOTE_SUBMIX, AudioDeviceInfo.TYPE_IP -> "remote"
        else -> "other"
    }

    private fun rank(type: String) = when (type) {
        "bluetooth" -> 4
        "usb", "wired" -> 3
        "hdmi" -> 2
        "speaker" -> 1
        else -> 0
    }

    /** Calls [changed] whenever an output comes or goes (Bluetooth connecting, headphones in); [stop] ends it. */
    class Watch(context: Context, private val changed: () -> Unit) {
        private val audio = context.getSystemService(AudioManager::class.java)
        private val callback = object : AudioDeviceCallback() {
            override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>?) = changed()
            override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>?) = changed()
        }

        init {
            audio?.registerAudioDeviceCallback(callback, Handler(Looper.getMainLooper()))
        }

        fun stop() {
            audio?.unregisterAudioDeviceCallback(callback)
        }
    }
}
