# Cratarr for Android

A thin app around Cratarr's pages, so a phone or a tablet can be a Cratarr player that Home Assistant and the other
players can command (Play On), also with the screen locked (decision [0031](../docs/decisions/0031-android-app-and-play-on.md)).

It is [Jellyfin's Android app](https://github.com/jellyfin/jellyfin-android) made into Cratarr's, and keeps its
licence, the GNU General Public License version 2 ([LICENSE.md](LICENSE.md)); the rest of this repository is
AGPL-3.0-or-later. Every file taken from Jellyfin's app says at its top what was changed for Cratarr, and when.

## What it does

- Shows the Cratarr server's pages (the address is asked once; Settings → Network change it). The pages come from the
  server; nothing of the web app is built into the app. Two addresses (decision 0031's amendment): the public one, and
  a local network address used while preferred and answering (`Reach.kt`).
- Plays the music itself (Media3's ExoPlayer in a media session service): the page's player hands its queue over, and
  the app fetches each track's original file (`/api/file/{source}/{id}`, with byte ranges) and plays it gapless. The
  lock screen, the notification, headphone buttons, audio focus, pausing when headphones go, and a wake lock and a
  Wi-Fi lock while music plays come with it. A page frozen out of view (Chromium freezes a hidden, silent page) does
  not stop the music or its controls.
- **Stay ready** (Settings): the service keeps the foreground while paused too, so Android does not put the app to
  sleep. For a device that is always on its charger.
- Settings: a long press on the app's icon.

## The page and the app

On the server's pages only, the app adds `window.CratarrApp` (`postMessage` and `onmessage`, JSON strings).
`web/src/lib/app.svelte.ts` is the page's side; `PagePlayer.kt` the app's.

| From the page | |
|---|---|
| `{"type": "hello"}` | The page uses the app; answered with `{"type": "app", "version", "player", "stayReady"}` and a player report. |
| `{"type": "player", "op": "queue", "tracks", "index", "positionMs", "play"}` | Play these tracks (the page's own: `source`, `id`, `name`, `artist`, `album`, `album_id`). |
| `{"type": "player", "op": …}` | `add` (`at`, `tracks`), `move` (`from`, `to`), `remove` (`index`), `jump` (`index`), `next`, `previous`, `seek` (`positionMs`), `play`, `pause`, `toggle`, `stop`. |
| `{"type": "settings"}` | Opens the app's settings. |
| `{"type": "open", "url"}` | Opens a link outside the app. |

| To the page | |
|---|---|
| `{"type": "player", "index", "positionMs", "durationMs", "paused", "error", "queue"?}` | What plays: on every change, once a second while the page is in view, with the queue when it changed (or the page said hello). |

## Building

Android Studio's JDK (21) and the Android SDK:

```sh
export JAVA_HOME="<Android Studio>/jbr" ANDROID_HOME="<Android SDK>"
./gradlew assembleDebug   # app/build/outputs/apk/debug/cratarr-android-debug.apk
```

The app carries its own copy of the page (decision 0085): build it first (`pnpm build` in `web/`), so `web/dist` is this
release's. Without it the app is built anyway and goes to the server's page, with a warning;
`./gradlew -Pcratarr.page=required assembleRelease` refuses instead, for an app that is handed out.

A release build is signed when `keystore.file`, `keystore.password`, `signing.key.alias` and `signing.key.password`
are given (Gradle properties or environment variables).

Jellyfin's later fixes can be taken over with `git subtree pull --prefix=android --squash
https://github.com/jellyfin/jellyfin-android.git master`, though most of its app is gone here.
