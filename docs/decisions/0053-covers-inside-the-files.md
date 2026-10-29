# 0053: Covers inside the files, and next to them

- Status: accepted (the owner, 2026-10-19)
- Date: 2026-10-19
- Builds on: 0013 (Keep and Replace), 0030 (covers for OpenSubsonic), 0033 step 4 (the app's downloads), 0052 (local
  mode)

## Context

On the phone, local mode showed broken images: the owner's own files had no cover anywhere, and the page drew a
picture that failed to load as a broken one (or, in some places, as an empty square). The owner then set the rule
for covers: "Covers should also always be embedded inside the files", and, asked whether that meant copies made for
the purpose: "No, I'm proposing no extra files. I want the already existing cover art to be embedded as well as saved
next to the content files of an album."

Measured that day (read only): of 2,255 album folders in Music, 84 had no cover file; of 24,527 tracks, 6 had no
picture inside (all of one album, restored from the backup in July).

## Decision

- **A cover lives twice**: inside every track, as the front cover, and as an image file next to the album's tracks
  (`cover.jpg`, `.png`). No copies or caches made for it.
- **The library**: brought there on 2026-10-19 with the owner's yes: the 84 folders got a cover file from the picture
  inside their first track, and the 6 tracks the folder's cover inside (ID3 version and every other frame kept, the
  audio byte for byte the same). What remains, the owner's "cleaning to do first", comes later.
- **New music**: Keep and Replace make the album's folder so before it moves into Music (`covers_inside`): a cover
  file from the picture inside a track when there is none, the folder's cover inside each track without one. Each
  track is written to a copy first and only replaces the original when its audio is byte for byte the same. FLAC and
  MP3 only; another format is left as it is, and logged. It never stops the move.
- **Files that leave Cratarr** carry the library's own cover, now inside them. The app's smaller copies (Ogg Opus,
  0033 step 4) get it as Ogg's comments carry one (`METADATA_BLOCK_PICTURE`); a cover above 300 kB goes in as a
  1000 px JPEG, so the copy stays small.
- **Where there is none at all** (a file from elsewhere, as the owner's own on the phone): the page shows a stand-in,
  never a broken image: Cratarr's record mark, faint, on a quiet tile (the owner chose it over a tile coloured per
  album; one look everywhere: `lib/Cover`).

## Consequences

- A Keep or Replace writes into the incoming album's files before moving them; its Undo moves them back with the
  cover inside (and the cover file with them).
- Smaller copies grow by their cover (at most a few hundred kB).
- The rest of the library's cleaning stays a separate step.
