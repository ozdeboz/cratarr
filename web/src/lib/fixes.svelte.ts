// The fixes Health offers for its library findings (the administrators' metadata workflow; the owner, 2026-10-26:
// Cratarr proposes, an administrator approves): reviewed on the one sheet (`fix:<finding>`, lib/FileChanges) and
// written as a run (decision 0092's writers' map, 0.14).

/** The findings with fixes. */
export const FIXABLE = new Set([
  'library:year',
  'library:genre',
  'library:album-artist',
  'library:musicbrainz',
  'library:rules',
  // A track's length (2026-10-28): read wrong, guessed, cut off, breaking off.
  'files:length-misread',
  'files:length-guessed',
  'files:cut-short',
  'files:sound-breaks',
  // A FLAC's broken seek table, made padding (0.14, the server's length fixes): offered nowhere before (the owner,
  // 2026-11-03: no sheet to be found).
  'files:bad_seektable',
  // Cratarr's words into the files (decision 0097), one finding per kind set to Write in Catalogue's "Written to files".
  'words:genre',
  'words:style',
  'words:mood',
  'words:tag',
]);
