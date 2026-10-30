# 0038: Kept on this device: playlists, crates, albums and artists that stay in step offline

- Status: accepted (the owner, 2026-10-16)
- Date: 2026-10-16
- Builds on: 0033 (Cratarr holds the queues; step 4, downloads in the app), 0024 (crates)

## Context

Downloads in the app (0033, step 4) copy an item's tracks once: a playlist downloaded today does not get the tracks
added to it tomorrow, and keeps the ones taken out. Next to them the app keeps the next five tracks of the queue and
lets them go once played.

The owner pointed at Symfonium's "Configure auto offline cache" (the artist menu) and asked how it would look for us:
items the device keeps in step by itself.

## Decision

**Keep on this device** is a switch on a playlist, a crate, an album or an artist (the owner, 2026-10-16).
- While it is on, the app follows the item: a track added to it is downloaded, a track that leaves it is removed from
  the device. An album rarely changes; an artist's new album arrives by itself; a crate follows its conditions.
- **Crates are the rules.** Symfonium's separate auto-cache rules (recent favourites, most played, …) are what a crate
  already says; keeping a crate on the device is a rule-based offline cache, with nothing new to learn.
- **In the UI the switch replaces Download.** **Download stays in the context menu**, as a snapshot: the item's tracks
  as they are now, kept even when they later leave it.
- Download is not Export: Export (0007, 0025) is the browser's zip of playlists and files for a computer, a stick or a
  DJ program; Download and Keep hold tracks inside the app for offline play.

**A track goes only when nothing holds it.** One held by two kept items, or by a manual download, stays until none
holds it. The queue's next five (0033) stay as they are.

**Limits.**
- One storage budget for the device: **20 GB by default**, settable higher, up to no limit.
- An optional cap per kept item (the first N in its order, or the newest N).
- When the budget runs out, the item kept longest ago shrinks first (its tracks furthest from its start). Manual
  downloads and the queue's next five are never dropped for it.

**When it syncs.** In the background: when the app starts, when Cratarr says a kept item changed, and every few hours.
- Over Wi-Fi only unless mobile data is allowed (the existing setting).
- **Only while charging**: a switch, **off by default**.
- In the quality the device is set to: the files as they are, or Cratarr's smaller copies (Opus).

**Settings → Sync** (a part of the settings, in the app; the owner): the budget, Wi-Fi only or mobile data too, only
while charging, and the quality, which moves there from "This device".

**A progress sheet, not a list on the page** (the owner). While anything syncs or waits, the Downloads page's top bar
shows a small progress ring; a tap opens the sheet (so does the item menu's switch). It lists each kept item with its
state ("Chill: 112 of 140", "waiting for Wi-Fi", "done", "3 failed, retry"), its size and a way to stop keeping it;
then the track downloading now, and the storage used against the budget. With nothing to do the ring is gone.

**Cratarr's side.** The lists exist already (a playlist's, a crate's, an album's and an artist's tracks). Each kept
item needs a cheap change marker (a version or an ETag), so the app asks "anything new?" without fetching the whole
list; crates change by themselves (a heart, a new album). Play On's live channel can say which kept items changed.

## Consequences

- Downloading becomes mostly unattended: the owner keeps a crate such as "favourites of the last 30 days" and the
  device follows it.
- The app's store counts holders per track; removing a kept item removes only what nothing else holds.

## Amendment (2026-10-24): the Downloads page shows it, as Finamp's

The owner, after looking at Finamp's Downloads page: take more from it, in Cratarr's look. The progress sheet goes;
the page itself says what is on the device and why.

- **Names.** The switch is **Sync to this device**; the context menu's snapshot is **Download once** (it fills its row
  as the tracks come in, the menu staying open). Export has its own icon.
- **Laid out as Insights, Health and Settings** (the owner, the same day): rows in groups, each with its picture or
  icon, a line and ›; a tap opens the item in the panel, beside the list on a roomy window (one always open, its row
  tinted), over it on a phone.
- **This device**: the tracks, their size against the budget (the row fills as far as it is used) and the quality,
  › Settings → Sync; what waits ("Waiting for Wi-Fi", or how many); what failed, with Retry.
- **Synced**: each item with its picture, kind, count and size or state; its row fills while it syncs. Its own menu
  (in the panel) has the switch.
- **Downloaded once**: the albums of the one-time copies (the app marks each download `manual` or not), filling as
  they come. A track a synced item holds too shows under Synced only (the owner, the same day); it stays a one-time
  copy, so it is back under Downloaded once when that item stops syncing.
- The pill's button syncs now, ringed while anything syncs or waits.
