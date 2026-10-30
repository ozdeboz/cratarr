# Play On probe (decision 0031)

Can a page on a tablet be told to play, pause and resume from the server, with nobody touching it and the screen
locked? A page that takes commands over server-sent events and reports what happened, to measure that in a browser
and again in the Android app.

1. `python -I probe.py static 8090`: serves the page and the commands.
2. On the device: open `http://<this machine>:8090/probe/` (or install it as an app), then lock the screen as the test
   needs.
3. Send `GET /probe/cmd?c=play` (or `pause`, `resume`); read what the page did at `GET /probe/log`.

Result 2026-10-15 (Lenovo Tab, Chrome, installed as an app):

| Case | Result |
|---|---|
| Not tapped since the app started | the command arrives, `play()` is refused (`NotAllowedError`); a tab is the same |
| Tapped, screen on | works |
| Locked while playing | stays connected; a pause after 37 s worked, the heartbeat went on |
| Locked, paused under a minute | resume works |
| Locked, paused longer | the page froze about 75 s after the pause, the connection dropped, a command 2 min later reached nobody; back only once the screen came on |

YAACC (a native UPnP renderer) on the same tablet was still reachable after 5 minutes stopped and locked, without a
tap, but is not gapless (`SetNextAVTransportURI` fails with 701).

Result 2026-10-15, the same through Cratarr's Android app (step 1 of 0031: autoplay allowed, a foreground service
holding a wake lock and a Wi-Fi lock, Stay ready on):

| Case | Result |
|---|---|
| Not tapped, also behind the lock screen | `play()` works |
| Locked while playing | works; pause after 40 s and resume work |
| Locked, paused under a minute | resume works |
| Locked, paused longer | the page froze 70 to 90 s after the pause, with the app's wake lock held and neither process frozen by Android: Chromium freezes a hidden page that is silent. A command 3 min later reached nobody. |
| Then the screen woken, the lock screen showing | still frozen: the page stays hidden behind the lock screen |

The page logs Chromium's `freeze` and `resume` events (shown once it resumes).

Result 2026-10-15, the app playing natively (Media3, 0031 amended; not through this probe: Chill sent to the app's
player over the bridge, its state read from Android's media session): played from 1:55 of track 1, the screen locked,
went on to track 2 by itself, paused by a media button; 8 min 19 s later, the screen dozing, a media button resumed it
at once. The page being frozen meanwhile no longer matters.

So a browser cannot be a Play On target: the app needs autoplay allowed in its WebView, and a foreground service with
a wake lock that also holds while idle.
