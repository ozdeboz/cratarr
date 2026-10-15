# Cratarr for Android

A thin app around Cratarr's pages, so a phone or a tablet can be a Cratarr player that Home Assistant and the other
players can command (Play On), also with the screen locked (decision [0031](../docs/decisions/0031-android-app-and-play-on.md)).

It is [Jellyfin's Android app](https://github.com/jellyfin/jellyfin-android) made into Cratarr's, and keeps its
licence, the GNU General Public License version 2 ([LICENSE.md](LICENSE.md)); the rest of this repository is
AGPL-3.0-or-later. Every file taken from Jellyfin's app says at its top what was changed for Cratarr, and when.

## What it does

- Shows the Cratarr server's pages (the address is asked once; Settings change it). The pages come from the server;
  nothing of the web app is built into the app.
- Keeps the page playing with the screen locked: a foreground service with the lock-screen and notification controls,
  a wake lock and a Wi-Fi lock while music plays, audio focus, and a pause when headphones are unplugged.
- **Stay ready** (Settings): the service and its locks stay while idle too, so a Play On command after a long pause
  still arrives. For a device that is always on its charger.
- Settings: a long press on the app's icon.

## The page and the app

On the server's pages only, the app adds `window.CratarrApp` (`postMessage` and `onmessage`, JSON strings):

| From the page | |
|---|---|
| `{"type": "hello"}` | The page uses the app; answered with `{"type": "app", "version", "player", "stayReady"}`. |
| `{"type": "state", "id", "title", "artist", "album", "cover", "position", "duration", "paused", "canSeek"}` | What plays (times in ms). |
| `{"type": "stopped"}` | Nothing plays any more. |
| `{"type": "settings"}` | Opens the app's settings. |
| `{"type": "open", "url"}` | Opens a link outside the app. |

| To the page | |
|---|---|
| `{"type": "command", "name", "value"}` | `play`, `pause`, `next`, `previous`, `stop`, `seek` (value: ms). From the lock screen, the notification, headphones and audio focus. |

## Building

Android Studio's JDK (21) and the Android SDK:

```sh
export JAVA_HOME="<Android Studio>/jbr" ANDROID_HOME="<Android SDK>"
./gradlew assembleDebug   # app/build/outputs/apk/debug/cratarr-android-debug.apk
```

A release build is signed when `keystore.file`, `keystore.password`, `signing.key.alias` and `signing.key.password`
are given (Gradle properties or environment variables).

Jellyfin's later fixes can be taken over with `git subtree pull --prefix=android --squash
https://github.com/jellyfin/jellyfin-android.git master`, though most of its app is gone here.
