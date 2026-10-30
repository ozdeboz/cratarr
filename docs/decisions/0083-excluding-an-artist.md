# 0083: Excluding an artist: not in this crate, or everywhere

- Status: accepted (the owner, 2026-10-30, for 0.10)
- Date: 2026-10-30
- Builds on: 0024 (crates; amended 2026-10-19: tracks left out of a crate, tracks excluded everywhere), 0061 (the
  filter), 0069 (Discover), 0026 (the one undo)
- Amended by: the amendment of 2026-10-30 below (the Artist filter itself counts every credit)

## Context

Until now only tracks could be excluded: everywhere, or left out of one crate. Discover's "Not interested in
<artist>" reaches only Discover. Leaving out an artist meant leaving out their tracks one by one, and a featured
artist or a collaboration slipped through.

## Decision

The owner's, 2026-10-30:

1. **Not in this crate uses the crate's own artist filter.** It adds "Artist is not X" to that crate's own filter, as
   one would set it by hand: the name joins an "Artist is not" row that is on (of any track), else a row of its own.
   Once: an artist the crate leaves out already, in any spelling of their key, is not added again. It is matched as
   the artist filter matches (then the artist and album artist tags, whole; by any credit since the amendment below).
   - One's own crate gets it in its saved conditions; its undo is the crate's.
   - A fixed crate has no filter of one's own: the permanent Unlisted, a genre's (style, mood, tag), Home's (New for
     you, Grave digging), and every Sounds like (Sounds like your week, the moment's, a track's, an album's, a
     playlist's suggestions). It gets a list of added conditions per account, kept with the account's crates
     (`crates.json`, `added`) by the crate's id (a Sounds like's without its count, as its fit), applied with its own
     conditions by the same matcher (`crates::passes`/`select`). Its undo is a change of its own (`Added`).
   - One way for both: `POST /api/crates/{id}/not-artist {name}`, `DELETE /api/crates/{id}/not-artist/{key}`; the
     crate's view (`GET /api/crates/{id}`) lists them as `not_artists: [{key, name}]`, and shows the rows among its
     conditions.
2. **Exclude everywhere is one list per account**, in the user data (`excluded_artist`: account, key, name, since
   when), read by every crate (the fixed ones too), Radio, Autoplay and the shuffles, Subsonic's random songs,
   Discover, and Home's New for you, Grave digging, Blast from the past and moment. Played by hand, they play, as an
   excluded track does.
   - The key is the name's loose key (`import::loose`, as New for you's three per album artist: case, accents, a
     leading "the" and a "feat." part count for nothing).
   - **Any credit counts.** A track crediting the artist anywhere (main, featured, a collaboration) is left out: its
     artist and album artist tags and its artists list, each credit split where a credit list joins artists ("&", ",",
     ";", " / ", brackets, "feat.", "ft.", "featuring", "x", "vs.", "with"; the tags keep these inside a value, 0008).
     An album is left out when its album artist is them, by the same reading.
   - One reading, once per pick: the account's keys are read once (`UserData::excluded_artists`), each file's credits
     once with the crates' facts (`Facts::credits`), and every pick asks the same question (`credits_out`,
     `credits_excluded`). A crate carries it unseen, as its left-out list (an axis no one builds with).
   - `PUT /api/me/excluded-artists/{key} {name}`, `DELETE /api/me/excluded-artists/{key}`,
     `GET /api/me/excluded-artists` → `[{key, name, at}]`. Each change has its undo.
3. **Discover's "Not interested in <artist>" stays apart**, Discover's only, as it was, with its own undo and its own
   list (`GET /api/discover/excluded`, `DELETE /api/discover/excluded`). Settings › Listening lists both kinds and
   takes each back by its own route.
4. **Search shows the artists excluded everywhere as left out**, as Discover's left-out artists are, a kind of their
   own (`excluded-everywhere:<key>`, "Excluded everywhere").

## Consequences

- Every pick that leaves out a track excluded everywhere now also leaves out what credits an artist excluded
  everywhere; a crate's counts, cards and search's crate members agree with its rows.
- A crate's "Artist is not" row is the filter's own: the Filter sheet shows and edits it like any other row.
- A short name excludes only itself: "Beck" leaves out "Air feat. Beck", never "Becky Hill". A part of a joined credit
  does count ("Hall & Oates" for "Oates").
- A crate's rows carry their album's size (`album_tracks`) on Cratarr's own source, beside its `album_id`, for the
  Records view's "3 of 10". Rows on another server have neither: the catalogue keeps no server's album ids, and asking
  the server for them per page is not cheap.

## Amended 2026-10-30 (0.10): the Artist filter counts every credit

The owner, 2026-10-30: the Artist filter itself now counts every credit, so "Not in this crate" and "Exclude
everywhere" match the same way. One meaning wherever the axis is used (the Library's filter, a crate's conditions,
every list's Filter sheet, the crate builder): a row matches a file's credits (`Facts::credits`) by the same loose key,
whole names only. "Artist is Beck" finds "Air feat. Beck" and "Beck & Air", never "Becky Hill"; "Artist is not Beck"
leaves them out; "AC/DC" stays one name. The axis offers and counts each credited artist, as first written
(`Facts::credit_names`), so a featured track counts for both. The stored row is unchanged; only matching widens.
An artist's own page and their Top recommendations still go by their artist and album artist tags.
