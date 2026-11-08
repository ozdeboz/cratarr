# The measuring day: the page (decision 0102)

The page's part of the gate: whether a phone's browser keeps the one engine sounding, and how it hears and answers,
before the page's normal play moves onto it. The app's part is [README.md](README.md).

The measuring page is `web/e2e/measure-engine.html`, served by the dev server only, never built into the page. It plays
today's page head at a steady 1x, one track round and round, three ways:

- **engine**: the head straight to the speaker;
- **element**: the head through an `<audio>` element, for background play and the lock screen;
- **plain**: the browser's own `<audio>` playing the same file, the baseline.

## Setting up

The head's AudioWorklet runs only in a secure context, so the phone opens the PC's dev server as `localhost`:

1. On the phone, turn on Wireless debugging (Developer options) and pair it with the PC. The pairing code is typed by
   the phone's owner, never by a session. Wireless, so the phone is not charging while its battery is measured.
2. On the PC: the dev server (`pnpm dev`, port 5400, `CRATARR_URL` at the server whose music it plays), then
   `adb -s <phone> reverse tcp:5400 tcp:5400`.
3. On the phone's browser: `http://localhost:5400/e2e/measure-engine.html`. With `?track=<source>/<id>` it plays that
   track of Cratarr's (signed in there through the dev server); without it, a generated tone.

A real FLAC track is the right one for the battery: a generated WAV costs almost nothing to decode.

## What to do

| # | Measure | How |
|---|---|---|
| 1 | Background play | Each way in turn: Start, lock the phone, leave it 30 minutes, unlock, read the report. |
| 2 | Controls | While each way plays: pause and play from the lock screen, a headset's button, Bluetooth. |
| 3 | The element's delay | Measure the element's delay, in a quiet room, the phone's own speaker on, nothing else sounding. |
| 4 | Time to first sound | Each way's first Start of a track not opened before. |
| 5 | An hour's battery | Unplugged, screen off: an hour on the better of engine and element, then an hour on plain. |

## What the report says

The page keeps its report in the browser (`Copy the report`, or `window.measure.log()` over the browser's remote
debugging):

- `minutes`, `soundMinutes`: time gone by, and sound heard in it. Less sound than time is a gap.
- `gaps`: each with its time, how much was missing, and whether the page was hidden then. With the screen off, the
  page's timers run once a minute, so a gap is counted over that minute.
- `contextStates`, `visibility`: the audio context's and the page's own changes (`suspended`, `interrupted`, hidden).
- `presses`: each Media Session action that came, and whether the page was hidden then. None from the lock screen
  means the browser gave that way no lock-screen controls.
- `firstSoundMs`, `fetchMs`: from Start to the first sound heard, and to the file fetched (the head's ways).
- `batteryPerHour`: the battery's fall per hour while not charging (where the browser tells the battery).
- `delay`: by the microphone, the engine's path and the element's (`engineMs`, `elementMs`) and how much later the
  element is heard (`extraMs`). Where the element is the way, the heard place takes that extra.

The gate's questions for the page: does any way keep sounding with the screen off (no gaps over 30 minutes), does it
get lock-screen controls, and is its battery within reach of plain's.
