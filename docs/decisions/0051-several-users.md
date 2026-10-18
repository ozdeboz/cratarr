# 0051: Several users

- Status: proposed (the design agreed with the owner, 2026-10-19; building waits for their go)
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
