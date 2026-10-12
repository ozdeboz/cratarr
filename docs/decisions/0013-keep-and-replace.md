# 0013: Keep and Replace: Incoming albums into Music

- Status: accepted
- Date: 2026-10-13
- Builds on: 0010 (writable libraries, the trash), 0011 (track identity, events, ports)

## Context

Incoming lists what Lidarr fetched, with a verdict per album: new, an upgrade of an album in Music, or a duplicate
of one. Delete is done (trash in the Incoming folder, Lidarr stops fetching, Undo). Two actions remain: Keep (a new
album goes into Music) and Replace (an upgrade takes the place of the album in Music). Lidarr is not responsible for
the collection: Music is the user's own, named the user's way.

On the reference install, 2,230 of 2,231 Music folders are named `Artist - Album (Year)` with the original release
year, and 1,993 of 2,000 sampled files `NN Artist - Title` (`D-NN …` on multi-disc albums), with a `cover.jpg`.
Lidarr names Incoming `Artist/Album (Year)` with its own file names. Some Incoming albums are hi-res (24-bit, 96 kHz
and more); the owner wants Music at 16-bit/44.1 kHz.

## Decision

- **Keep** moves a new album from its Incoming folder into the Music folder, named per the Music library's naming
  template: folder and file names, not tags. Tags stay as they are.
  - The template is a per-library setting (Settings → Libraries), pre-filled by analysing the library's existing
    folders and files. The reference install's: `{albumartist} - {album} ({originalyear})` /
    `{disc-}{track:02} {artist} - {title}`.
  - The year is the original release year (`originaldate`, else `date`). When neither is known, the confirm step asks
    for it once.
- **Replace** puts an upgrade in the place of the album in Music. Each new file is paired with an old track
  (`incoming::pair`, decision 0011 §4: MusicBrainz recording, then disc/number and title, then title and length, then
  disc/number and length), and takes over its track id, so playlists and play data follow it. New tracks with no old
  one (bonus tracks) are added. **Replace is refused when an old track has no match**: the owner keeps the old album
  whole rather than lose a track or mix formats.
- **Hi-res is converted to 16-bit/44.1 kHz FLAC on the way in**, in Cratarr, in pure Rust: decode (symphonia),
  resample (rubato), TPDF dither to 16 bit, encode (flacenc). The image stays small and any install can do it. Before
  it is trusted, its output is compared with SoX's on albums from the reference library; the comparison is recorded
  here. It runs in the background with progress; the album waits in Incoming until done.
- **Lidarr stops fetching** an album that was kept or replaced (unmonitor through the acquisition port), as on Delete.
  Otherwise Lidarr sees it gone from its folder and fetches it again.
- **Replaced old files go to the trash in the Incoming folder** (`.cratarr-trash`, decision 0010): the same disk as
  Music on the reference install, so an instant move, and outside the Music folder, so the Music backup never carries
  them. Kept for the trash days set in Settings.
- **Undo** reverses Keep and Replace right after, through the journal (decision 0011 §6): files back, track ids back,
  Lidarr monitoring back.
- **Events**: Keep announces `AlbumAdopted`, Replace `AlbumReplaced`, each with every track's move (old path → new
  path, its track id). The playlist module rewrites `.m3u` lines and Jellyfin entries from them (its handler); the
  media-server port copies play data (plays, favourites) from the old item to the new one.

## Consequences

- Music keeps one naming convention whatever Lidarr does, and the template is the one place it is defined.
- A replace that would lose a track does not happen; the album stays an upgrade in Incoming until a complete copy
  arrives or the owner deletes it.
- Conversion quality is Cratarr's responsibility; the SoX comparison is the gate.
- The order of work: Keep (no conversion: refuses hi-res until conversion lands), then Replace, then conversion.
