# Plans

Big ideas that are not decided and not being built. Each entry gives what it is, why it waits, and the catches known so
far. A plan that is taken up becomes a decision in [decisions](decisions) first, and its entry here points to it.

Most of these wait for the same reason. Cratarr is a **thin client**: the server holds the library, the queues (0033)
and the pages, and the app shows the server's pages, with a native player under them (0031). The app keeps tracks for
offline play (0038), but it only knows what the server tells it. Each plan below asks the phone, or a car, to know
something the server doesn't.

## External library

Another music server's library played in Cratarr's app directly: an OpenSubsonic server, a Jellyfin, through the same
kind of adapters Cratarr's own sources use (0005).

- Waits for: the thin-client model. Today another server's music reaches the app only through Cratarr's server, as one
  of its sources.
- Catches:
  - The app would need its own adapters, its own sign-in for each server, and a library view that isn't a server page.
  - Queues, Play On and the history are held by Cratarr's server (0033). A track that only the phone can reach can't be
    handed over to another player, and its plays are not counted where the rest are.
  - Crates, playlists and analysis (0024, 0002, 0035) work on what the server catalogues. An external track would have
    no tempo, loudness or waveform unless the phone analysed it.

## Local library

The phone's own music files, played in Cratarr's app beside the server's.

- Taken up (2026-10-19) as [0052](decisions/0052-local-library.md): a local mode of the app, never mixed with Cratarr's
  music. What follows is the plan as it stood.

- Waits for: the thin-client model, as above.
- Catches:
  - Reading the phone's files needs Android's media permission and an index of its own on the phone.
  - The same split as the external library: queues, Play On, history and crates live on the server, the files on the
    phone. Mixing both in one queue means a queue the server can't fully play elsewhere.
  - Not the same as kept tracks (0038): those are the server's tracks copied to the phone, and stay the server's.

## CarPlay

Cratarr's player in the car, on an iPhone.

- Waits for: an app on iOS. Cratarr has an Android app only (0031).
- Catches:
  - CarPlay shows an audio app through Apple's own list and now-playing templates, not a web page. The thin client's
    pages can't be its face in the car: the browsing would be native.
  - An audio app on CarPlay needs Apple's CarPlay entitlement for it, asked for and granted per app.
  - Android Auto, the counterpart on Android, was taken out of the app with the rest of Jellyfin's extras (0031).
    It is designed again and waits for a car or head unit that runs it (0055: the owner's tabs, voice and start; the
    car's JVC unit does not), and would come before iOS: Android Auto browses through a media library service
    (Media3), the kind of service the app's player already runs in.

## A chosen output while following a group

This device's chosen output (0033, amended 2026-10-19) applies when it plays alone or leads a group, not when it follows
one: then Snapcast's own player makes the sound, and the Snapdroid build Cratarr ships (v0.29.0.2) opens its Oboe stream
without a device. The owner (2026-10-19): "for later, if ever".

- What it would take, as looked at then:
  - The change is small: snapcast's `client/player/oboe_player.cpp` (v0.29.0) builds its stream with no `setDeviceId`;
    passing the `--soundcard` option's index there (and the app giving it the chosen output's id, restarting the player
    when the choice changes, as it does for a group change) is about ten lines.
  - The work is building snapclient ourselves instead of taking Snapdroid's: its build is Gradle with CMake 3.22.1 and
    NDK 27.2, snapcast as a submodule, and eight prebuilt dependency AARs from badaix/snapcast-deps v0.29.0 (boost,
    flac, oboe, ogg, opus, soxr, tremor, vorbis). Building arm64-v8a only would do for the tablet and the phone.
- Catches: GPL-3.0, so the patch stays in the repo with the source offer; our own build to keep up with Snapcast (or the
  patch offered upstream); a test by ear on a device following a group.

## One similarity for playlists and crates

Radio, Autoplay and "more like this" each found similar music their own way from a playlist, a crate or a track
(0036's profile: shared playlists, artists, genres, tags, era and AudioMuse's sound, 0054). The owner (2026-10-23): "we
should consolidate playlist and crate similarity into a unified module or path later on. We'll handle this after all
AudioMuse analysis is done."

- Done in 74.x (2026-10-23): one path, `similarity` (signals, rarity, middle and spread, the listener's Similar by),
  for the radio, Sounds like (a track's, an album's, your week's), and the similar albums, playlists and crates at a
  list's foot; the radio and the crates read a track from the same facts (`crates::Facts::like`).
- Left, and it waits for AudioMuse's analysis of the whole library (with CLAP): weighing the signals (all equal now,
  or the listener's order), CLAP text search, steering the radio by mood and energy, the closeness graph last.
- Seen so far: a crate's Start radio seeds from 300 of its tracks (drawn by the server since 73.2) and leaves the seed
  out; a broad crate (Unlisted, 16,000 tracks) makes a blurry profile. Play as radio (only the crate's tracks) is a
  different path again.
