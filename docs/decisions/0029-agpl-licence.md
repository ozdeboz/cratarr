# 0029: The AGPL licence

- Status: accepted
- Date: 2026-10-15

## Context

The repository said MIT from its first server commit (2026-10-12), but the owner had never chosen a licence, and
Cratarr was not yet published. The owner is considering making Cratarr an OpenSubsonic server itself, in place of
Navidrome. The strongest open projects to learn from and reuse are copyleft: Navidrome, gonic and LMS (GPL-3.0), and
nghe, an OpenSubsonic server in Rust (AGPL-3.0). Under MIT none of their code could be used, translations included.

## Decision

Cratarr is licensed under the **GNU Affero General Public License, version 3 or any later version**
(`AGPL-3.0-or-later`), chosen by the owner on 2026-10-15.

- Code from GPL-3.0 and AGPL-3.0 projects may be used and adapted, with its copyright notices kept.
- Dependencies stay under licences that may be combined with it (MIT, Apache-2.0, MPL-2.0 today).

## Consequences

- Whoever runs a modified Cratarr for others over a network must offer them its source; running it unmodified, or
  for oneself, asks nothing.
- Nobody can make a closed version of Cratarr, nor build it into closed software.
- Changing the licence later needs every contributor's consent once others contribute.
- An iOS version through Apple's App Store would be hard; F-Droid and Google Play accept the AGPL.

## Amendment (2026-10-15): the Android app keeps Jellyfin's licence

The Android app starts as Jellyfin's Android app (jellyfin/jellyfin-android), imported into `android/`. That app is
GPL-2.0 and says nothing of later versions, and its many contributors cannot all be asked, so it cannot become AGPL:
`android/` stays GPL-2.0, with its own `LICENSE.md`, and the rest of the repository stays AGPL-3.0-or-later. One
repository may hold both: they are separate programs that talk over the network, which both licences treat as
mere aggregation.

- No code moves between `android/` and the rest, in either direction (GPL-2.0-only and AGPL-3.0 code cannot be
  combined into one program).
- The app loads Cratarr's pages from the server, as Jellyfin's app does; the web app is not built into the APK.
- Jellyfin's copyright notices stay; Cratarr's own changes to the app are GPL-2.0 as well.

