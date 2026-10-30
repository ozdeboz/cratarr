# What's new in 0.11

Since 0.10.0; 0.10.1's fixes are in it too. The notes for each part are in [decisions](decisions).

## Quick crates

- **A crate from a few words.** Type them in the field at the top of Crates, or search and tap **Make a crate of
  this** at the top of the results. Each word (commas between them) becomes a chip that shows how it was read: a
  year or a decade first, then the best match, a genre, an artist, a label or an album. A tap on a chip shows its
  other readings, and Remove.
- **An artist means "sounds like".** Their chip switches between Sounds like and By. Sounding like an artist leaves
  their own tracks out, unless **Their own tracks** keeps them.
- **How alike.** With an artist to sound like, a curve shows how many tracks are how alike; its handle moves the cut.
  It starts close. Saved, the crate's condition reads "at least N% alike".
- **At once.** The crate shows its count and its first tracks while you type. **Play** and **Shuffle** play it as it
  is (its first 500), **Save** makes it a crate of your own and opens it, **Edit** opens it in the crate builder.
- **Kept until saved.** The last five unsaved quick crates stay on the device, offered while no words are typed.
- Quick crates show only when the server makes them.

The closeness curve counts among the tracks the crate's other words let through, so its numbers match the crate's.

## Search and the library

- **Labels** have a group of their own in search. A label opens its crate, as a genre does: the files' label tags,
  else the labels Discogs already gave.
- **Other versions, marked.** In an album's tracks, a track with other recordings of its piece carries a quiet mark
  after its title; a tap opens its menu at Other versions.

## Pages that know what the server can do

- **The server lists what it can do** (`GET /api/capabilities`, signed in): what this version's code does and what is
  set up here. The page asks once, keeps the answer for offline, and leaves out what the server can't do
  ([0085](decisions/0085-offline-first.md), step 1).
- **The Android app carries its own page.** It opens with the server out of reach, and the newer of the two pages is
  the one used (each page's `build.json`, `/api/health`'s `web`).

## Under the hood

- **The page is served from a folder,** not from inside the server's binary: `CRATARR_WEB_DIR`, else the image's
  `/web`. A fix to the page alone no longer means building the server again.
- **The rules in Rust in the Android app**, a spike behind a switch that is off: the tape's brake from the shared
  rules crate, compared with the app's own. Its answers match; a call costs about 100 µs, so the one tape engine
  ([0075](decisions/0075-one-tape-engine.md)) will cross into Rust once per frame, never per value.

## Fixed in 0.10.1

- Next shows the new cover at once: the next and previous covers are ready ahead.
- Holding Previous or Next: the waveform's window stays centred on where letting go lands, stops at a track's start
  and end, and keeps its width at top speed; the current track keeps one colour.
- A hold back never comes out playing forward, the knob always agrees with what plays, and holding back while playing
  backwards speeds it up backwards.
- The tape deck grows up to three times its size on a large landscape screen.
- The desktop app quits when asked, and an update restarts a running copy.
