package app.cratarr

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import org.json.JSONObject

/**
 * Commands for the page, from the lock screen, the notification, headphones and audio focus: `play`, `pause`,
 * `next`, `previous`, `stop`, `seek` (with the position in ms). The activity passes each on to the page that said
 * hello, as `{"type": "command", "name": …, "value": …}`.
 */
object PageCommands {
    private val flow = MutableSharedFlow<String>(extraBufferCapacity = 32)
    val commands: SharedFlow<String> = flow

    fun send(name: String, value: Long? = null) {
        val message = JSONObject().put("type", "command").put("name", name)
        if (value != null) message.put("value", value)
        flow.tryEmit(message.toString())
    }
}
