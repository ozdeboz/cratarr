# 0084: Each user's own music

- Status: accepted as a plan (the owner, 2026-10-30), for 0.13 (after 0.11 quick crates and 0.12 modes per server); not built
- Date: 2026-10-30
- Builds on: 0044 (folders), 0051 (several users), 0070 (requests), 0080 (single-track intake and its gate),
  0058 (Download…)

## Context

Cratarr has one Music library, Lidarr's Incoming, and accounts that may ask for albums (0070), with no limits. The
owner wants users to be able to want music or upload their own, into music of their own, without the server storing
the same album twice, and wants every user to be able to export their library.

## Decision (the owner's, 2026-10-30)

**Three kinds of library.** Music (the main one), Incoming (Lidarr's, the owner's), and **each user's own music**:
a library of its own, a real folder on the NAS. What a user gets through Cratarr goes there directly. It is called
their own music, not incoming. There is no new "saved" mark: the Favourite (the heart) stays the only mark.

**Seeing.** Every user can switch every library and every Incoming on or off for themselves; switching only hides from
one's own view. Defaults: every library on; one's own music on; the owner's Lidarr Incoming on (open to everyone);
other users' own music off. Radio and Autoplay's choice of libraries (0.9.5), the crates' library switches and the
Subsonic apps' music folders already work per library, so each user's music joins them as a library. With 0.12's
modes per server, these switches stay within Cratarr's own mode (the owner, 2026-10-30: "Switches only within
Cratarr"): Music, Incoming and users' own music; other servers become modes of their own.

**What arrives, and how.**

- **Wants** go straight in while the user has room in their allocation (no approval). Lidarr (or Soulseek) fetches as
  today; Cratarr runs the single-track gate's checks on the album (whole decode, fake lossless, clicks, clipping),
  moves it from Lidarr's area into the requester's own music, and unmonitors it in Lidarr so it is not fetched again.
  A fetch that fails the checks stays in Lidarr's area for the owner, and the user is told why.
- **Uploads** from the page or the apps, from anywhere (signed in), in resumable pieces, no cap per upload; audio
  files and covers only. They are handled apart from Lidarr (Lidarr's folder is Lidarr's alone): checked as above,
  matched by tags, MusicBrainz and AcoustID, then put into the user's own music, straight in within the allocation.
- **Already stored elsewhere** (in Music or another user's music): the album is put into the user's own music as a
  btrfs **shared-block copy** (a reflink): a real folder entry, no extra disk until one copy changes. Cratarr shows the
  copies as one album where it lists the whole library.

**The allocation.** A budget in GB per user (100 GB by default, set per user in the Users panel). It counts what
their own music takes on disk; **shared-block copies are free** (they cost the NAS nothing). Wants and uploads stop
with a clear line when it is full.

**A floor for the disk.** Below 200 GB free on the music volume (set in Settings), users' wants and uploads pause
("The server is short of space") and the owner is alerted once through Home Assistant's notify, and again when it
recovers. The owner's own fetches go on.

**Rights in one's own music.** A user may delete albums from their own music (to the trash, with undo) and use the
album fixes on it (tags, covers, renumbering, replacing a track), never on Music or other users' music. Moving an
album into Music stays the administrators'. Administrators keep every right everywhere.

**Export.** A user exports their own music plus their favourites from libraries they don't own, as favourited (a
favourite album whole, a favourite track as itself in its artist/album folder, a favourite artist's albums), through
a **read-only network drive per user** (WebDAV) that Cratarr serves: "My music" and "Favourites", signed in with the
account's app password, home network only, copied natively by any computer and resumable. Windows' own client wants
HTTPS for passwords, so it is reached through the home https name. **From outside, per account:** a switch on each
account, "Network drive from outside", off by default, opens that user's drive over the public internet too, over HTTPS
through the public name, signed in the same way and through the sign-in throttle. It needs Caddy to pass the drive's
path to Cratarr's own login check, as it does the Subsonic API's `/rest` (nas-infra).

**A user who leaves.** Their own music stays as it was, an ownerless library that counts against nobody, until an
administrator acts.

**What exists today.** Stays where it is: Music and Incoming are not moved or reassigned; users' own music starts
empty. Requests still open when this ships are fetched into their requester's own music.

**The NAS** (nas-infra): users' music is neither in the cloud backup (STACK) nor in Jellyfin.

## Consequences

- Wants and uploads need no administrator: the allocation and the disk floor are the brakes; the owner sees users'
  music only by switching it on.
- One album may exist in several folders (shared-block copies); Cratarr lists it once, and a fix in one copy does not
  reach the others.
- Users' music has no off-site backup; uploads that exist nowhere else are lost with the NAS. The owner chose this.
- Built in parts: users' libraries, their switches and the allocation; wants routed into them with the checks; the
  disk floor; uploads with matching; the export drive.
