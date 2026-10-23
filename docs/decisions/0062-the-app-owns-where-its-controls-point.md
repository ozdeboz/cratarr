# 0062: The app owns where its controls point; sounding here means controls here

- Status: accepted (the owner, 2026-10-23: "we need a more robust architecture. This is a serious smell"; chose
  "decision first", then build and test on the phone before a release)
- Date: 2026-10-23
- Builds on: 0031 (the app and Play On), 0033 (Cratarr holds the queues; amended 2026-10-23: one screen, one place),
  0037 (sync play)
- Replaces, in the app: the page's `prefs.output` as the truth, `hereAgain` and `watchOwn` (lib/playOn), and the
  `drive` message with `Prefs.driven` (0031, amended 2026-10-23)

## Context

A device's controls point at one place: its own player, one of Cratarr's players it drives, or the media server's
player. In the app that fact had two owners. The page kept it (`prefs.output`, in the WebView's storage) and told the
app with messages; the app's player could start playing by itself (Play On, the car, the lock screen, a reconnect, an
install that restarted it), and the page noticed only a start it saw happen. On 2026-10-23 the phone played its own
music while its page and Android's media controls pointed at the Salon tablet: the controls reached the tablet, and
nothing reached the music that sounded ("It's stuck playing"). Each fix so far (`hereAgain`, `watchOwn`, the session's
swap) was one more place keeping two copies in step.

## Decision

**One owner.** In the app, where this device's controls point (its **target**: `self`, `player:<name>`, or `server`)
is kept by the player service (`PlayerService`, `Prefs.TARGET`) and changed only there. Everything else shows it:

- the media session (its own player for `self`, a `RemotePlayer` for `player:<name>`; 0031, amended);
- the page: it reads the target from the app as it opens (the `device` answer to its `hello`) and on every change
  (`{"type": "target", "target", "why"}`), and asks for a change with `{"type": "target", "to"}`. In the app the page
  keeps no output of its own;
- the notification, the lock screen, the output chip and the volume keys, through the session.

**One rule, enforced where playback starts:** *sounding here means controls here.* The service watches its own
player; the moment it plays (whatever started it) while the target is another, the target becomes `self`, before
anything else, and the page is told why. Pointing away from `self` happens only with its own player quiet: the page
moves the music first, as it does (Take over: its queue to the other player and its own stopped; Don't take over: its
own paused), and the service, asked to point away while its own still plays, pauses it first, whatever the page did.
A group it follows (0037) is sound here: the target is `self`, and the page shows the group's leader, as now.

**Written down.** Every change of target goes to the app's link log with its reason (the page asked, its own player
started, a restart).

**In a browser** there is no app and one owner already: the page (lib/playOnTarget, lib/output), where its own
player is started only by the page or by a Play On command, which turns the screen here first. That stays as built.

## Consequences

- The `drive` message, `Prefs.driven`, and the page's `hereAgain`/`watchOwn` in the app go; the page's output in the
  app is the app's target as last told.
- A reload, a reinstall or a reconnect cannot leave the two apart: the page asks the app, and the app's rule holds by
  itself.
- Tested on the phone before a release, each of: playing, then moved to the tablet; playing, then "Don't take over";
  pointed at the tablet, then Play On to the phone from elsewhere (Home Assistant, another device); the app restarted
  (an install) while pointed at the tablet and playing; the page reloaded while pointed at the tablet. (Pointed at the
  tablet, the lock screen, a headset and a car control the tablet: they reach the session.)
