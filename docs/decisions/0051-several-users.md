# 0051: Several users

- Status: accepted (the owner, 2026-10-19: "Go.")
- Date: 2026-10-19
- Builds on: 0030 (OpenSubsonic and its accounts), 0031 (Play On), 0033 (Cratarr holds the queues), 0039 (sign-in and
  sessions), 0040 (the directory asked first), 0043 (sign-in everywhere)

## Context

Cratarr has one owner. The OpenSubsonic API already serves each of its accounts their own favourites, plays, ratings,
bookmarks and play queue: the user data store keeps them by user, and so do the visuals' snapshots, ratings and learned
limits. Cratarr's own API does not: about thirty places act for "the owner" (the first administrator), whoever signed
in. Crates, playlists and the settings have no owner at all. Since 0040 people sign in with the directory's password,
but an account still maps to one Cratarr account by hand.

The owner (2026-10-19): "we need to support multiple users, their own settings".

## Decision

- **Accounts from the directory.** Every member of the directory's `music` group signs in to Cratarr with their own
  account, made at their first sign-in; members of `media_admin` are administrators (the owner: "Yes, to the music group
  for Cratarr accounts"). Cratarr's own account stays the fallback administrator. Each user sets their own password for
  OpenSubsonic apps (whose login needs the password itself, which a directory does not give).
- **Each user's own:** favourites, plays and ratings, bookmarks, queues, the history and Insights, Radio's memory (what
  was played lately), the visuals' snapshots, ratings and learned limits, and their own settings. Cratarr's own API acts
  for whoever is signed in, as the OpenSubsonic API already does.
- **Playlists: personal, with a share option** (the owner: "I do want personal playlists with a share option"). A shared
  playlist is seen by every user, who can play it and copy it into their own; only its owner changes it, and it shows
  whose it is. A user's playlists are theirs by name, so two users can each have one called the same. Playlists are
  Cratarr's alone: music is no longer managed in Jellyfin, so they are not kept in step with Jellyfin's per user.
- **Crates: personal, not shared** (the owner: "Crates personal but without share option"). The built-in crates are
  everyone's.
- **Devices: each plays only on their own** (the owner: "Users can only play on their own devices"). A device belongs to
  the account it signed in with; Play On lists one's own; a sync group is of one's own devices; a Kodi player belongs to
  the administrator who added it. Home Assistant's key acts for the owner, so the household's devices go on as they are.
- **Administrators only:** the server's settings (library, folders, conversion, sources, Lidarr), reviewing what comes
  in, the trash and moves, API keys, sign-in's settings and the users.
- **The settings split** in two: the server's (the administrators') and each user's own (Radio's and Autoplay's
  defaults, leaving favourites out, and the like).
- **Moving over:** everything there is becomes the owner's; their playlists start unshared.

## Consequences

- The thirty-odd places that act for the owner act for the signed-in user; the pages hide what is the administrators'.
- Playlists and crates move from one store to one per user (how they are kept on disk is settled when building).
- A shared device (the salon tablet) is the owner's: others play on their own phones.
- The Jellyfin playlist sync is not carried per user; whether it goes from Cratarr altogether is a separate decision.
- The app signs in per user as it does now; tracks kept for offline play are kept per user.

## Access, decided with the owner (2026-10-19)

- **Losing access:** membership of the `music` group is checked again every hour (the owner: "it will hardly ever
  happen. Maybe every hour is fine"); one who left loses their sessions, the app's sign-in and their OpenSubsonic
  password, and the passwords kept for when the directory is down stop working for them.
- **All music for everyone** (the owner: "All users should see all music"), what is still coming in too.
- **Changes to the library are the administrators'**: tags, covers, the album fixes, rejecting an album, deleting and
  moving files. Users keep their own favourites, ratings and playlists.
- **Asking for new music** (Lidarr, the discovery, downloads) is not offered to users yet.
- **Now playing shows one's own** (the owner: "now playing shows only your own").
- **A Users panel**, as Jellyfin's (the owner: "a user panel with a permissions section ... It's probably how Jellyfin
  does it too"): each user's profile (name and role, from the directory), their permissions (among them what
  OpenSubsonic apps may do for them: keep tracks to play offline, make and share playlists, public share links; and
  their app password), and their devices and sign-ins, each to see and end.
- **Listening is private:** administrators do not see other users' listening history.
- **API keys** have an owner and a scope (reading, or control), and only administrators make them.
- **One who leaves** is deactivated, their data kept.
- **Repeated wrong passwords** lock the account for a while.
- **The change journal records who did it for the essential changes only** (the owner: "Keep the change journal
  essential only"): what deletes, moves or changes the library.
- **Sync play admits only its own players** (the owner: "Yes, the fix should go in"). Its port on the local network
  (0037) took any device that named a player, to listen to a group or, as its leader, to play into it. A player is given
  a secret of its own when it joins a group, and the sync server turns away a listener or a leader without it.

## Amendment, 2026-10-19: as built

- **Who a request is for.** Each request runs as its account: a sign-in fully; an API key as its owner (a key from
  before keys had one: the first administrator), reading or controlling as made; an OpenSubsonic app as its login,
  controlling. An administrator's routes take an administrator signed in, never a key or an app. While signing in is
  not required, one who signed in (or sends a key) is still that account; anyone else is the first administrator, and
  everything is theirs as before.
- **Devices.** A player is the device of the account it first connected or reported with, and a name is one account's
  player: another account's device under that name is refused (`taken`), and the app and the page take the next free
  name. The players from before accounts are the first administrator's; a Kodi is the administrator's who added it.
  Requests reach only their own players: the list, the commands, joining and handing over at both ends, the player
  behind what plays now, the groups' visuals. Cratarr's own work sees every player. Home Assistant's webhooks carry the
  first administrator's players, as its key acts for them.
- **Playlists on disk.** The first administrator's stay where they were, in the playlist folder; another account's are
  in a folder of its own there (`.playlists/<account>`, made with its first playlist), their lines relative to Music as
  everyone's. An account knows its own by name, so nothing that was there changed its id, and another's shared one as
  `owner/name`. Cratarr's own records of a playlist (when its entries came, its old names, its sharing) are keyed the
  same way. Album moves follow into every account's playlists. Only the first administrator's are kept in step with
  other servers, and only their names are spoken to the voice assistant.
- **Undo** takes back one's own changes only: the journal keeps whose each change is.
- **Crates** are kept per account (`users/<account>/crates.json`); the built-in ones are everyone's, and their pictures
  are the administrators' to change.
- **One's own settings** are kept on the server (`/api/me/settings`), as the apps keep them, for every device of the
  account. The server's settings stay the administrators'.
- **Permissions**: without "offline" an OpenSubsonic app may not download, and without "playlists" it may not change
  playlists (the roles it is told say so); without "share" one may not share one's playlists. Public share links are
  not offered yet, so "links" waits for them.
- **Sync play**: a player is told its secret with its group; its snapclient's id is `secret:name`, and a leader's sound
  comes with it. A Kodi listens through signed links, as before.
- **The journal** keeps who did each change to the library (a queued one: who queued it) and who undid it.
- **Wrong passwords** are counted per client and per account, the account's name in any case, on the sign-in page and
  for OpenSubsonic apps alike.
- **Not covered yet**: deleting an album stops it first only on the deleting administrator's own player; another
  user's device finds it gone at its next track.

## Amendment, 2026-10-19: Cratarr's own accounts beside the directory's

The owner: "LLDAP shouldn't be the only way for us to manage users in Cratarr, since not everybody would use Cratarr
with LLDAP"; of the proposal, "Yes, we do it the way you proposed."

- **Two kinds of account, one Users list.**
  - Cratarr's own: made in Users by an administrator.
  - The directory's: when one is set up, made at the first sign-in, as above.
  - Permissions, devices, sign-ins, privacy and what is one's own are the same for both. Only where the name, the
    password and the role live differs.
- **Adding one.** An administrator gives its name, a password, and whether it administers.
  - The first account there is administers, whatever is chosen (nghe's setup).
  - The person signs in with it and changes the password themselves (Account → Password).
  - A Cratarr account's password is also its OpenSubsonic one. The app password stays for directory accounts.
- **A Cratarr account's page.**
  - Its role. Never one's own, and never the last active administrator's.
  - A new password.
  - **Remove**: as leaving the directory, the account is switched off, its data kept and its sign-ins ended.
    **Restore** switches it on again. Nothing is deleted for good.
- **A directory account** stays read-only here: its name and role are set in the directory.
- **Names** are unique across both kinds, case set aside.
- **The fallback administrator.** Cratarr's account from before (`subsonic-users.json`) is one of Cratarr's accounts in
  Users. Authentication no longer has an account group of its own; it keeps the sign-in requirement, the API keys
  and the directory.
- **Borrowed.**
  - The server's account routes are translated from nghe (vnghia/nghe, AGPL-3.0: `route/user`: create,
    update_role, update_password, setup), with its notice.
  - Navidrome's user screens (GPL-3.0-or-later) are the checklist of rules.
  - Jellyfin's Users panel (GPL-2.0-only) is the design only: no code.
