/*
 * Where the music sounds (decision 0033, amended 2026-10-19): the device Android routes media to now, told to Cratarr
 * with each report, so a queue remembers where it was heard ("the car queue") and one started in the car can take the
 * car's name. Read from Android's own routing (no permission asked); a change (Bluetooth on or off, headphones in or
 * out) is reported at once.
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
