# 0055: Android Auto, designed and waiting for a car that runs it

- Status: accepted as a design, not built (the owner, 2026-10-19: the car's JVC unit does not run Android Auto, so
  71 keeps the car on Bluetooth; this is built when a car or head unit with Android Auto comes)
- Date: 2026-10-19
- Builds on: 0031 (the app; Android Auto went out with Jellyfin's extras), 0033 (queues), 0024 (crates), 0038 (kept
  tracks)

## Context

The owner asked: "can we prepare for Android Auto in 71?" Today the car is Bluetooth audio only: the phone's player
plays over the JVC head unit ("JVC Unit"), which does not run Android Auto (the owner, asked). Around it:

- Queues are named by where they played (68: `played_on`), so the car has its own queue ("play the car queue again").
- HA starts music when the phone connects to the JVC unit (`automation.car_music_when_the_phone_connects_to_the_jvc`;
  the owner: "as soon as my phone is connected to the JVC unit, I want music playing"), and hands the car's music to
  the salon on arriving home.
- The app's player is a Media3 `MediaSessionService` (`PlayerService`, 0031, amended), with no browse tree.

What Android Auto asks of a media app (Android for Cars, read 2026-10-19):

- **A media library service.** Auto connects to a `MediaLibraryService` (or `MediaBrowserService`): a browse tree of
  browsable items (folders) and playable items (leaves), search, and a session to control. The manifest declares the
  service with the `android.media.browse.MediaBrowserService` action and an `automotive_app_desc` resource that uses
  `media`.
- **At most four tabs.** Only browsable items at the root show, as tabs, "in most cases … four"; each with a
  monochrome icon and a short label. Each level shows a limited number of items, and there is no paging.
- **Artwork from the phone only.** Covers reach Auto as local URIs (`content://` from the app's own content provider,
  which fetches and caches the remote picture; `android.resource://` for icons), not as web addresses. Ours need the
  session cookie anyway: the Downloads loader already sends it (66.1).
- **Voice.** "Hey Google, play X on Cratarr" arrives as a search for the app to answer and play.
- **Sideloaded apps are hidden.** An app not from the Play Store shows in Auto only after Auto's developer mode
  (tap its version ten times), Unknown sources in its developer settings, and the app ticked in Customize launcher.
- **Testing** without the car: Google's Desktop Head Unit on the PC, with Auto's head unit server on the phone.

## Decision

When it is built:

- **`PlayerService` becomes a `MediaLibraryService`.** The same player, session and Play On link; it adds the browse
  tree, search and the car's entries in the manifest. The 69.1 lesson holds: a connection's commands come from
  `AcceptedResultBuilder(session, controller)` with ours added explicitly, never from the default result, and the
  library callbacks are checked the same way.
- **Four tabs, in the owner's order: Crates, Playlists, Favourites, Queues.**
  - Crates: each crate as an item that shuffles it (as its card does).
  - Playlists: each playlist as a folder of its tracks, playable from the top.
  - Favourites: the favourite tracks (shuffled) and the favourite albums.
  - Queues: the queues by where they played, the car's first; a tap takes one up.
  - Lists are kept short (the most recent, or the pinned first), since Auto neither pages nor shows long lists.
- **Voice: albums and artists first** (the owner), then tracks, then crates and playlists by name.
- **On connecting to Android Auto, the car queue resumes** (the owner): the app takes up the queue last played in
  the car and plays on this phone. HA's Bluetooth car automation stands down while Auto runs, so the two do not both
  start it.
- **Covers through a content provider** of the app's, which loads them with the Downloads loader (the session cookie)
  and caches them.

## Consequences

- Nothing is built in 71. When a car or unit with Android Auto comes, this is the plan: the app's library service, and
  anything the server needs for short browse lists; tested on the Desktop Head Unit first.
- The owner sets Auto's developer mode, Unknown sources and the launcher tick once on the phone.
- HA's car automation needs a guard ("not while Android Auto runs") at the same time.
- docs/plans.md's CarPlay section points here for Android.

## Sources

- Android for Cars, media apps: https://developer.android.com/training/cars/media
- Build the content hierarchy (four tabs, no paging):
  https://developer.android.com/training/cars/media/create-media-browser/content-hierarchy
- Display media artwork (local URIs, a content provider):
  https://developer.android.com/training/cars/media/create-media-browser/media-artwork
- Sideloading on Android Auto (developer mode, Unknown sources, Customize launcher):
  https://www.androidauthority.com/sideload-apps-on-android-auto-3681820/,
  https://tomsguide.com/phones/android-phones/this-hidden-setting-changed-the-way-i-use-android-auto-and-im-never-going-back
