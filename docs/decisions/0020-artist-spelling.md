# 0020: An album goes into the folder Music already has, whatever its capitals and accents

- Status: accepted
- Date: 2026-10-14
- Builds on: 0013 (Keep and Replace), 0016 (Music's layout)

## Context

Since 0016, Music has one folder per album-artist credit, named from the tags as written. An album that comes in
through Lidarr often spells the same artist differently: "DARKSIDE" for the "Darkside/" that Music has. Named by the
template alone, Keep would make a second folder beside the first. On Linux the two are different folders; on a disk
that ignores case (SMB to Windows or macOS, most backups) they are one, and one of them is hidden or overwritten.

## Decision

- **Keep and Replace spell each folder of the new path as Music already spells it** (`incoming::follow_spelling`):
  where Music has exactly one folder at that level whose name is the same ignoring capitals and accents, that folder
  is used. "DARKSIDE/Spiral (2014)" goes into "Darkside/".
- **The album's own folder follows too.** An album that Music has under another spelling is then found by the
  existing check, and Keep refuses with "Music already has a folder named …" instead of making a case twin.
- **Only capitals and accents.** "Dark Side" is not "Darkside": a space or a dot can be a different artist, and the
  owner decides those by editing tags.
- **Music's spelling wins over the tags.** The files keep their tags as they came; only the folder follows Music.
  The file names still come from the tags (`{artist}` in the template), so a track can read "DARKSIDE - …" inside
  "Darkside/".
- **Several existing spellings, or none:** the rendered name stays as it is.

## Not done

- Matching by MusicBrainz artist id (a renamed artist, "Prince" and "The Artist…"). The catalogue holds the ids, but
  which folder an id lives in is not a single answer in a library that has several credits per artist; a name match
  covers the case that collides on disk.
