# 0087: Offline writes: ids, times and the change log

- Status: accepted (the owner, 2026-10-30, with the shapes agreed between the server and the page's offline work);
  built for 0.12
- Date: 2026-10-30
- Builds on: 0085 (offline first, step 3 as amended: the change log and playlist edits as operations in 0.12), 0033
  (plays with ids), 0064 (every track heard), 0051 (several users, shared playlists), 0002 (the `.m3u` files are the
  truth), 0.9.15's `Idempotency-Key` (a change sent again gives its first answer)

## Context

0085's step 3: nothing done offline is lost. The page keeps what is done while the server is out of reach in an outbox
and sends it later, perhaps days later, perhaps twice, perhaps after another device changed the same thing. Finamp and
Symfonium are held back there by servers without ids per entry, times or a change log. Cratarr's server is its own, so
it gives them. What the server must give: each change made once however often it comes, an older change never
undoing a newer one, a way to hear what changed since, and playlist edits that merge instead of overwrite.

## Decision

**The change's own time is a header.** Every outbox write carries `X-Cratarr-At: <epoch ms of the change on the
device>`. Read in one place (an extractor), not per handler; no body or query field. Without it the change is made
now. The page writes through the endpoints it already uses, not new ones:

- favourites: `…/track/{source}/{id}/favorite?on=`, `…/albums/{source}/{album}/favorite?on=`,
  `…/artists/{source}/favorite?name=&on=`, `…/playlists/{id}/favorite?on=`;
- ratings: `…/albums/{source}/{album}/rating?stars=`;
- playlists: `…/track/{source}/{id}/playlists/{p}?member=`, `…/playlists/{id}/append`, `…/playlists/{id}/rename`,
  and their operations (below);
- `/api/excluded`.

**Favourites and ratings: the last change wins.** The server keeps the time of the last change per account, kind and
item. A write whose time is older than that changes nothing. Its answer is the endpoint's own (the state as it now
stands) with `"applied": false`; one made answers `"applied": true`. The other endpoints may ignore the header. A
Subsonic app's star and rating, and Home Assistant's heart, are changes made now.

**Sent again, done once.** The `Idempotency-Key` the page already sends with each change (its operation's id) is kept
with its answer for 30 days, in the user data, not only in memory for a day: the same key from the same account for
the same change gets the first answer back, across a restart too. There is no second dedup beside it.

**Plays.** As 0033 had them: each with an id and the time it was played, sent late and again until answered, counted
once. The ids counted are kept in the user data for 30 days from when they came (a player gives a play up after 30
days), checked before counting; the set in memory stays as a cache.

**The change log.** One counter per server, never reused. A row per change: `(rev, account, kind, item, op, at)`, the
latest only per item (an item's earlier row goes when it changes again). `GET /api/changes?since=<rev>&kinds=a,b` →
`{rev, changes: [{rev, kind, id, op}], more}`, at most 1,000 per answer, the asking account's own and the playlists
shared with it; `more` says ask again from the last one's `rev`. Without `since`, or with one older than what is kept
or newer than the server's counter: `{rev, reset: true}` (read everything again, then go on from `rev`). Kept 30
days or 50,000 rows, pruned in the background. Kinds and their ids:

- `playlist`: the playlist's id as the account names it (its own by name, another's shared one `owner/name`);
  `changed` (made, edited, renamed to it, shared or no longer) or `deleted` (deleted, renamed away). A shared
  playlist that stops being shared, or is deleted or renamed away while shared, is a `deleted` for every account
  other than its owner, under its own row (`owner/name`), so a later change of it does not hide that; the page
  ignores deletes of ids it does not hold;
- `favourite`: `track:<source>/<id>`, `album:<source>/<id>`, `artist:<source>/<name>`, `playlist:<id>`; `changed`
  is a favourite now, `deleted` no longer;
- `rating`: `album:<source>/<id>` (a Subsonic app's on a song: `track:<source>/<id>`); `changed`, or `deleted` at no
  stars;
- `excluded`: `tracks` (the tracks excluded everywhere) or `artists` (the artists excluded everywhere); `changed`.

Crates come later (0085: a crate's rules are edited online only until the change log covers them).

**Playlist edits as operations.** `POST /api/playlists/{id}/ops` with
`{ops: [{op_id, at, op: "add", entry, track: {source, id}, after} | {op_id, at, op: "remove", entry} |
{op_id, at, op: "move", entry, after}]}` (`after`: an entry or null, the start) →
`{rev, entries: [{entry, track}], refused: [{op_id, why}]}`, the playlist as it now is (`track` null where the line
is not a track on that server, `?source=` as the rows take it) and the operations refused (empty when none; `why` a
short phrase, "not found" for an add whose track cannot be found). The page drops a refused operation and says so in
one line.

- Every line has an entry id. The page makes an added line's; the server gives each other line one, kept beside the
  `.m3u` in the user data (playlist, entry, order). The server's own edits (the whole list put back, append, repeats
  taken out, membership, repairs, operations) keep them; a file changed from outside is matched again, by order and
  content, when it is next read.
- A playlist's rows (`GET /api/playlists/{id}/rows`) give each row its `entry_id`; `entry` stays the line. A row
  without one: no operations for that playlist.
- Membership merges as a set: an add of an entry known (there, or removed) changes nothing; a remove of one not there
  changes nothing; an add after an entry since removed goes after that entry's last known neighbour, and after an
  entry never known, at the end.
- The order: the last move by its time wins; a move older than the entry's last move or add changes nothing.
- An operation's `op_id` is kept with the change log's lifetime, a refused one with why: one sent again is not
  applied again, and one refused is refused again, not tried again.
- Ownership and permissions as every edit's: one's own playlists only.

**The server says it can.** `/api/capabilities` names `offline_writes` (the header honoured and the keys kept),
`playlist_ops` and `changes`; a server without them gets the page's present behaviour (0085: graceful degradation).

## Consequences

- An outbox replays in any order and any number of times without losing a newer change or making one twice.
- The page can hear what changed since it last looked and read only that, instead of everything.
- The `.m3u` files stay the truth: entry ids are a record beside them, made again from the file when it changed
  outside Cratarr. Ids of a line rewritten outside (or by a repair) are new ones.
- A favourite or rating changed on another server outside Cratarr (in Jellyfin's own app) is not in the log.
