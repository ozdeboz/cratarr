# 0064: One listening history: a log of every track heard

- Status: accepted (the owner, 2026-10-23: "we need a proper history framework"; chose every track heard with skips,
  kept forever, starting empty)
- Date: 2026-10-23
- Builds on: 0022 (when a play counts), 0033 step 4 (plays a player tells late, each counted once), 0051 (accounts)
- Serves first: Home's Sounds like your week and Jump back in (75), the radio's recently played (0036)

## Context

What Cratarr knows of listening is one row per account and song (and album) in `annotation` (userdata.sqlite): a play
count and the **last** play's time, overwritten at every play. Plays come in by three ways: the app's own player
(`/api/tracks/…/played`, `PlayingService::count_play`), players that tell their plays late (`/api/plays`,
`app::plays`, each counted once by its id), and the Subsonic apps' scrobbles (`serve::queues::count_play`).

That is enough for the Subsonic apps and the pages' "plays" and "last played", and not for anything about time:

- Sounds like your week can only take the songs last played within seven days, each once: a track played twenty times
  weighs as one heard once.
- Nothing can say what was heard on a day, in a month or a year ago, or what was skipped: a skip leaves no trace.
- Jump back in reads the albums' and playlists' last play only.

## Decision

**One log.** A table `plays` in userdata.sqlite, a row for every track heard by an account: the song, when it
started, how many seconds were heard of how long it is, how it ended (`finished`, `skipped`, `stopped`), the player
and output it sounded on, and what started it (an album, a playlist, a crate, the radio, a queue). Rows are only
added. The log is kept **forever** and goes with its account.

**One way in.** The three ways above, and every new one, write through one function (a `history` module), which adds
the row and, for a play that counts (0022: most of the track heard), adds to `annotation` as today. `annotation` stays
for the Subsonic apps and the pages; the log is the record of what happened.

- The app's own player and Cratarr's players (Play On) tell every track change: the track left, how much of it was
  heard, how it ended. A player that was offline tells them late, each once by its id (0033 step 4).
- The Subsonic apps tell only what they count (`scrobble`, `submission=true`); those rows are `finished`, with nothing
  known of skips.

**One way out.** The history module answers what an account heard between two times, by song, album, artist, genre
or style; weighted by how often and, where asked, older listening counting less; skips as their own count; sessions
(a long gap starts a new one). Features ask it, and never read the table themselves.

**Starting empty.** The log fills from the day it ships; nothing is made up from `annotation`'s last plays. Until a
feature's window is filled, it says so or falls back as it does today (Sounds like your week: the last plays within
seven days).

## Consequences

- Sounds like your week can weigh its seed by how often each track was heard this week, older days less, and leave
  skipped tracks out; Jump back in can follow what was really played last.
- New: what was skipped and how far in, for the radio and Insights; "on this day", a month, a year back.
- The players change: each tells every track change, not only counted plays. The app's player and Play On's are
  Cratarr's own; the Subsonic apps stay as they are.
- A few thousand rows a month per listener; years stay small. An account's deletion deletes its rows.
- Two records of plays: the log and `annotation`. Only the history module writes both, so they agree from the day the
  log starts; plays before it exist only in `annotation`.
