# What's new in 0.14

Since 0.13.0; release/0.13's fixes after it are in it too. 0.14 is what preview.2 holds, with the owner's review of it
(2026-11-03). Soulseek in Cratarr, modes per server and scrobbling from the device are 0.15
([plans](plans.md)). The notes for each part are in [decisions](decisions).

## References and the intake

- **References for every record:** MusicBrainz, Discogs, Bandcamp and AcoustID, each saying whether it agrees, differs,
  was found or is missing, with its track lengths. Use this, Paste a link, Checked by hand and Confirm on the record's
  References; Health counts the records without one.
- **An intake card for a record waiting in Incoming:** where it came from (Bandcamp, Soulseek, Lidarr or a folder),
  its checks (a reference, AcoustID, its sound, one record, the folder's name, one Soulseek user, already in the
  library), and each track's snippet and wave. Health's "Records waiting for you" lists them and opens each card.

## One way to change files

- **Every writer on one path** ([0092](decisions/0092-one-way-to-change-files.md),
  [the map](writers-onto-0092.md)): Health's fixes, the intake, Renumber, the tempo, the covers, the length fixes and
  a single track added are each a plan to review, then a run in Changes to files with one Undo. Each field says how
  sure it is; a genre with several answers offers them.
- **A FLAC's broken seek table** is a file problem, and its fix rewrites the table as padding.

## Taste

- **One taste per account** ([0094](decisions/0094-taste-in-sequences-and-lists.md)): Best match on every list (records,
  artists, playlists) and every crate's first order; Your taste as Autoplay's and the radio's mode; skips keeping tracks
  away gently, not for good.
- **One radio sheet** wherever radio starts.
- **Second chance:** a built-in crate of tracks you skipped that fit your taste.
- **Energy and danceability** on the library's 0–100% in the profile, the filter's window lit on its bars.

## Crates, favourites and playing

- **Favourites are one truth:** an own crate's heart and disc are its conditions, the same as typing them; a new crate
  starts at All tracks. "Album" in conditions is "Record".
- **One track plays now** and keeps the queue on every player; a list of one is that one track; a Play that replaces
  the queue offers Undo.
- **The crate sentence reads a tempo:** "120-130 bpm", "over 120 bpm", "about 90 bpm".
- **A crate showing nothing says why,** the search one of the causes; its conditions are counted over the libraries
  switched on.

## Libraries

- **Only what's switched on:** every count, list and play uses the libraries switched on; Home's crates are the one
  exception.
- **A library's quality limit** ([0096](decisions/0096-libraries-equal.md)) in four steps; Move and Replace say what
  converts. Adding to Music asks where.
- **Retention:** a library can keep deleted records.
- **Words** ([0097](decisions/0097-words.md)): one store of every record's and track's words by kind, each with where it
  came from; a record's words a row per kind.

## Health

- **A record's own problems** on its page.
- **A count said once,** a files finding listing its own items and offering Review the fixes; findings name records, not
  albums.

## Moving about

- **One way to say nothing** ([0095](decisions/0095-one-way-to-say-nothing.md)): every list the search empties says so
  with one cause; a long-press says what hides part of a list; the folded search with words kept shows ! on the magnifier.
- **Search through every part** of Settings, Health and Insights.
- **The glass:** the panel's actions on its glass's extension, the open search taking the row, edit mode's buttons on
  the top right glass, the beside panel locking by a long-press on its grip and keeping its own history.
- **Play on** in the mini-player's place while nothing plays.
- **Choice controls** one row everywhere: one of many, several, a switch or a value.
- **Settings' Resets** from each section, opening its sheet.

## Fixed

- The false "no sound" on some MP3s.
- Resume shows only where there is something to resume.
