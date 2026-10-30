# 0013: Keep and Replace: Incoming albums into Music

- Status: accepted
- Date: 2026-10-13
- Builds on: 0010 (writable libraries, the trash), 0011 (track identity, events, ports)

## Context

Incoming lists what Lidarr fetched, with a verdict per album: new, an upgrade of an album in Music, or a duplicate
of one. Delete is done (trash in the Incoming folder, Lidarr stops fetching, Undo). Two actions remain: Keep (a new
album goes into Music) and Replace (an upgrade takes the place of the album in Music). Lidarr is not responsible for
the collection: Music is the user's own, named the user's way, and never one of Lidarr's root folders (decision 0016).

On the reference install, 2,230 of 2,231 Music folders are named `Artist - Album (Year)` with the original release
year, and 1,993 of 2,000 sampled files `NN Artist - Title` (`D-NN …` on multi-disc albums), with a `cover.jpg`.
Lidarr names Incoming `Artist/Album (Year)` with its own file names. Some Incoming albums are hi-res (24-bit, 96 kHz
and more); the owner wants Music at 16-bit/44.1 kHz.

## Decision

- **Keep** moves a new album from its Incoming folder into the Music folder, named per the Music library's naming
  template: folder and file names, not tags. Tags stay as they are.
  - The template is a per-library setting (Settings → Libraries), pre-filled by analysing the library's existing
    folders and files. The reference install's: `{albumartist} - {album} ({originalyear})` /
    `{disc-}{track:02} {artist} - {title}`. Once Music is nested, it becomes one folder per album-artist credit:
    `{albumartist}/{album} ({originalyear})` / … (decision 0016).
  - The year is the original release year (`originaldate`, else `date`). When neither is known, the confirm step asks
    for it once.
- **Replace** puts an upgrade in the place of the album in Music. Each new file is paired with an old track
  (`incoming::pair`, decision 0011 §4: MusicBrainz recording, then disc/number and title, then title and length, then
  disc/number and length), and takes over its track id, so playlists and play data follow it. New tracks with no old
  one (bonus tracks) are added. **Replace is refused when an old track has no match**: the owner keeps the old album
  whole rather than lose a track or mix formats.
  - The whole old folder goes to the trash, its other files (cover, scans) too. The new album goes in at the
    template's folder, which may be another name than the old one.
  - A new file may take an old one's name: the old files move out first, and moving in that order is safe to
    repeat after a crash.
  - A year missing from the new tags comes from the old album's tags before the page asks for it.
  - Refused while an old track still has no track id (Cratarr is still reading the album), since nothing could
    follow it.
  - Undo gives every file its own track id back.
- **Hi-res is converted to 16-bit/44.1 kHz FLAC on the way in**, in Cratarr, in pure Rust: decode (symphonia),
  resample (rubato), TPDF dither to 16 bit, encode (flacenc). The image stays small and any install can do it. Before
  it is trusted, its output is compared with SoX's on albums from the reference library; the comparison is recorded
  here. It runs in the background with progress; the album waits in Incoming until done.
  - **Converted:** lossless files above CD format (`convert::needs_conversion`). Lossy files are left as they
    are; conversion cannot improve them.
  - **Checked first:** Keep or Replace checks its plan at once (year, names, track pairing, write access), so a
    refusal shows straight away. Only then does it queue the album.
  - **The queue:** one album and one file at a time, so the NAS stays free for the media server. It lives in
    `convert-queue.json` in the data folder and survives a restart. A file already converted is not converted
    again (the output is the same bytes, written whole or not at all).
  - **Where files go:** each file is converted into the Incoming folder's `.cratarr-convert`. One plan of moves
    then puts the originals in the trash and the converted files in their place. Then the action runs on what
    is now a CD-format album, and its event records what was converted.
  - **Undo:** puts the originals back (the converted files go to the trash). A failed action puts them back at
    once, so the album stays in Incoming as it was.
  - **Cancel:** possible while the album waits or converts; not once its files are being swapped in.
  - **The page:** the album's row shows "Waiting to convert" or "Converting n of N", and the Incoming tab shows a
    count. A toast says when an album is done or has failed, on whatever page is open.
- **Lidarr stops fetching** an album that was kept or replaced (unmonitor through the acquisition port), as on Delete.
  Otherwise Lidarr sees it gone from its folder and fetches it again. When no Lidarr is set in Settings, the confirm
  step says Lidarr may fetch the album again (seen on the first real Keep, 2026-10-13: the album stayed monitored).
- **The media server hears of the move** by album folder, and the library that gets a new folder looks for it at
  once. An album that is playing is stopped before its files move and goes on playing on its new item after
  (decision 0017).
- **Replaced old files go to the trash in the Incoming folder** (`.cratarr-trash`, decision 0010): the same disk as
  Music on the reference install, so an instant move, and outside the Music folder, so the Music backup never carries
  them. Kept for the trash days set in Settings.
- **Undo** reverses Keep and Replace right after, through the journal (decision 0011 §6): files back, track ids back,
  Lidarr monitoring back.
- **Events**: Keep announces `AlbumAdopted`, Replace `AlbumReplaced`, each with every track's move (old path → new
  path, its track id). The playlist module rewrites `.m3u` lines and Jellyfin entries from them (its handler); the
  media-server port copies play data (plays, favourites) from the old item to the new one.

## Conversion compared with SoX (2026-10-13)

`convert.rs`: symphonia decode; rubato FFT resampler with a long windowed-sinc filter (about 8k taps, delay
trimmed); plain TPDF dither with a fixed seed (no noise shaping); flacenc, streamed. Six tracks from four Incoming
albums (96/24, 192/24, 48/24, 44.1/24) against SoX 14.4.2 `rate -v -L 44100 dither`. Read-only copies were used.

- Below 20 kHz the outputs null to −98…−123 dB. The one −98 dB track comes from SoX clipping its own float
  reference.
- Durations are identical to the frame. Dither noise is −96.3 dBFS, the TPDF level, the same as SoX.
- Clipped samples: equal to SoX's or fewer. These are intersample overs in the masters.
- The only difference is the filter edge. This filter passes up to about 21.9 kHz; SoX `-v` rolls off from about
  20.9 kHz. The 21–22.05 kHz content matches the source's to within about 2 dB, and aliases are cut by more than
  100 dB (unit tests).
- Peak memory: 6 MB. Speed: about 15× real time on the NAS's N95, 4–5× slower than SoX. The same input always
  gives the same bytes.

Verdict: equivalent to SoX `rate -v -L` with TPDF dither. SoX's noise-shaped dither (`-s`) has a lower audible
floor; it was left out on purpose. Vorbis comments and pictures are carried over. Cuesheet and seektable blocks are
not, since their sample positions would be wrong after resampling.

## Consequences

- Music keeps one naming convention whatever Lidarr does, and the template is the one place it is defined.
- A replace that would lose a track does not happen; the album stays an upgrade in Incoming until a complete copy
  arrives or the owner deletes it.
- Conversion quality is Cratarr's responsibility; the SoX comparison is the gate.
- The order of work was Keep, then Replace, then conversion; all three are in place since 2026-10-13.
