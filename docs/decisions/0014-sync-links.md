# 0014: Sync links: one playlist, the targets it is kept in

- Status: accepted
- Date: 2026-10-13
- Supersedes: the "Playlists belong to" switch of 0012 (its sync engine stays)
- Builds on: 0002 (playlists are `.m3u` files), 0007 (export), 0012 (own Jellyfin playlists)
- Amended by: 0025 (links are set in the panel's Sync tab, for one playlist or several; the menu's Sync to… opens it); the amendment of 2026-10-15 below (no push: every account link is two-way)

## Context

0012 made every playlist the owner's private Jellyfin playlist at once, with one switch for the whole server. The
owner decided on 2026-10-12 that this is a per-playlist choice:

- A playlist can be *global*, visible to everyone, or *user-bound*: one account's own playlist.
- Syncing is a link per playlist to a target, with its own direction, conflict rule and status.
- One playlist can have several targets, set up from its menu ("Sync to…").
- Export (0007) stays a one-off download; a sync link lasts.

## Decision

- **One source:** every playlist is an `.m3u` file in `Music/.playlists` (lines carry `../`, as in 0012). It never
  moves; a target is a copy of it.
- **A sync link** is a playlist plus a target, a direction and a conflict rule, with a status (last sync, error).
  Links live in the sync state in the data folder.
- **Targets:**
  - **Shared:** a copy in the Music folder, which the media server imports for every user. Push only, because an
    imported playlist cannot be edited through Jellyfin (its API answers 403). The copy is rewritten whenever the
    playlist changes, and removed when the link goes.
  - **A user's Jellyfin playlist:** that account's own playlist, kept in step by 0012's engine (three-way merge,
    renames, the guards against mass deletion).
  - Later: a folder (a USB stick, another device's share) and other servers, as further kinds of target.
- **Direction:** push (the file wins; changes on the target are overwritten) or two-way. For two-way, the
  conflict rule picks the side that leads when both changed: the newest change (0012's merge), the file, or the
  target.
- **New on a target:** a playlist made in Finamp by an account that has a two-way link becomes a new playlist in
  Cratarr, with a two-way link to it.
- **A moved file is not a removal:** after a move (Keep, Replace, a restructure) the server lists the file's new
  item only once it has scanned it and Cratarr has linked it. A track a two-way target stopped listing while Cratarr
  has no server item for it stays in the playlist and goes back to the target once linked; a track taken out
  while linked is a real removal.
- **A new catalogue:** the lists both sides last agreed on hold track ids, which belong to one catalogue. When the
  catalogue is made afresh (a restructure, a lost data folder) they are forgotten, and the next pass merges both
  sides as they are: what either has stays, nothing is taken out.
- **Gone on one side:** a playlist deleted in Cratarr loses all its targets' copies. A copy deleted on a two-way
  target deletes the playlist (it can be restored in Cratarr). On a push target the copy is made again.
- **Restored:** a deleted playlist's links are put aside, not forgotten. Restoring it (its Undo) brings them all
  back, and the next pass makes the copies again. A new playlist of the same name starts fresh, Shared.
- **Taking a link off** deletes its copy on the target.
- **Migration, self-contained:** on the first pass after deploying, every `.m3u` in Music is copied into
  `.playlists` and gets a Shared link. The file in Music stays, as that link's copy: same path, same content, so
  nothing visible changes and the media server keeps the playlist (and its id) it imported.
  This runs once, recorded in the sync state, so the copies are never mistaken for playlists again.

## Consequences

- The owner chooses per playlist: global (Shared), user-bound (a user's Jellyfin playlist), both, or neither.
- Home Assistant's voice lookup finds playlists by name: a playlist with a user's Jellyfin target has a new id
  there.
- The data folder holds the links: they belong in its backup (as 0012 already says).

## Amendment (2026-10-15): two-way only, for whatever happens on either side

The owner decided that push goes. A push link quietly undid edits made in Finamp, which protects nothing anyone wants
protected: both sides are the owner's own playlist. (Tools that sync one way do so because their source is not edited
on the receiving side, a streaming service or a generated list; that is not this case.) Shared stays a copy the server
cannot edit, which is a limit of the server, not a choice: nothing comes back from it.

The owner also decided that the conflict rule goes: there is nothing worth choosing. Additions and removals from both
sides always merge; the rule only ever picked the order (or the name) when both sides changed it between two passes,
some 30 s. Which change came last cannot be told then: the server does not give a playlist's save time (Jellyfin's
DateLastSaved comes back empty), and both changes fell between two passes. So the server's leads, as the change seen
last. The link's direction and conflict-rule columns stay in the sync state so older files open; they are not read.

- **Every account link is two-way.** Links saved as push read as two-way; nothing else changes for them. The Sync tab
  is ticks only: a link has nothing to set.
- **An edit wins over a deletion** (the owner's choice): a playlist deleted on one side is deleted on the other only
  when the other is unchanged since both last agreed. Otherwise it comes back where it was deleted, with the edits:
  in Cratarr with every link it had (its Shared copy too), on the server as a new copy.
- **Names Cratarr cannot use** (the owner's choice): a server playlist whose name is taken here, used twice on the
  server, or not a usable file name, comes in under the nearest free usable name ("Chill 2", "AC-DC-"); the server copy
  is then renamed to match, so both sides agree. Nothing is merged with a playlist of the same name.
- **Changed on both sides between two passes:** the server's order, and the server's name when both renamed (see
  above). A server rename to a taken name gets the nearest free one here, and the server follows.
- **A track whose file is away for now** (an unmounted folder, a rescan under way) is no removal: its line stays as a
  missing one, and the server copy keeps it (the server-side rule above, the other way round). The catalogue drops a
  missing file's server links at once, so each link remembers the server item of every track both sides agreed on;
  with it, taking the missing line out here is a real removal there too, and the file coming back changes nothing.
- **One playlist's problem is its own:** the other playlists of the account go on, and the Sync tab shows the problem
  on that playlist's link; it is tried again next pass. Only a problem with the whole account (the listing, a listing
  not believed) stops the account's pass.
- **A change the server did not stamp** is found anyway: every tenth pass (about five minutes) reads every linked copy
  on the server whatever its change stamp says.
- **A renamed server copy keeps its heart.** Jellyfin renames with an API key by making a new copy and deleting the old
  one; the heart is carried over. Another linked playlist already holding the new name is never taken over: the
  rename waits, and the Sync tab says why.

## Amendment (2026-10-15, later): no links to set; shared means in everyone's account

The owner found per-playlist links implicit and confusing (a Finamp playlist only came into Cratarr once some other
playlist was linked to that account), and the imported Shared copy wrong for sharing: nobody can edit an imported
playlist in Jellyfin. Decided, with the owner's choices:

- **Your account syncs, always.** Per server, "your account" is the account chosen in Settings, else the server's
  administrator (stable: it does not change with who holds the followed player). Every playlist has an editable copy
  there, and every playlist made there (in Finamp) comes into Cratarr. Nothing to set per playlist; no links.
- **Shared means in everyone's account.** A shared playlist has an editable copy in every account on the server, each
  kept in step two-way with the one playlist in Cratarr. A playlist not shared is in your account only, and that is
  where every playlist starts (the owner: "they're mine only"): sharing is chosen per playlist.
- **Someone else deleting their copy** of a shared playlist takes it out of their account only, and it stays out for
  them (until it is shared again). Only you delete a playlist for everyone: in Cratarr, or in your own account.
- **Other people's own playlists stay theirs.** Their accounts sync only what is shared; their own playlists come into
  Cratarr once Cratarr has sign-in per person (a decision of its own).
- **Cratarr's copy stays out of Jellyfin's reach,** in `Music/.playlists` (Jellyfin skips hidden folders). No copy is
  written into Music any more.
- **Migration:** every playlist (all 66 were Shared) becomes yours only: none is put in other accounts. Each one's
  imported copy in Music is removed once its editable copy exists in your account, and your heart on the imported
  playlist moves to it. Copies are made a few per pass, so Jellyfin is not flooded (66 take a few minutes). Other
  accounts no longer see them (all of the owner's devices use his account). Finamp downloads of those playlists need
  downloading again (new playlist ids). The same holds for a first start that adopts playlists found in Music.
- **Reads are spread:** each pass reads about a tenth of the copies whatever their stamp says, so every copy is read at
  least every ten passes without a burst.
- The Sync tab becomes one switch, "Shared with everyone", with each account's copy and any problem it has.
