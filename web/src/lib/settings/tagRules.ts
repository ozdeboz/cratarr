// Tag splitting (decision 0008; GEN-DESIGN §4, OQ-GEN-22: each library's own since 0.18): how a library's files write
// several artists or genres in one tag, edited as text on Libraries › <library> › Tag splitting, and the default for new
// libraries and own music on the Libraries page. The aliases stay the install's (Named otherwise).
import type { TagEncoding } from '../api';

/** Why "&" can't separate genres (OQ-GEN-21), as the server says it when it refuses one. */
export const AMPERSAND = '"&" sits inside genres, as in Drum & Bass and R&B, so it can\'t separate them';

/** Genre separators as typed (separated by spaces): the reason they are refused where one holds "&"; none where fine. */
export const genreSeparatorsRefused = (typed: string): string | null => (typed.split(/\s+/).some((s) => s.includes('&')) ? AMPERSAND : null);

/** An encoding as its editors show it: the separators separated by spaces, the never-split lists a line each. */
export type SplitText = { artistSeps: string; genreSeps: string; artistKeep: string; genreKeep: string };

const words = (text: string) => text.split(/\s+/).filter(Boolean);
const lines = (text: string) =>
  text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);

export const textOf = (e: TagEncoding): SplitText => ({
  artistSeps: e.artist_separators.join(' '),
  genreSeps: e.genre_separators.join(' '),
  artistKeep: e.artist_keep.join('\n'),
  genreKeep: e.genre_keep.join('\n'),
});

/** The text as the server keeps it, in its fields' order. */
export const encodingOf = (t: SplitText): TagEncoding => ({
  artist_separators: words(t.artistSeps),
  genre_separators: words(t.genreSeps),
  artist_keep: lines(t.artistKeep),
  genre_keep: lines(t.genreKeep),
});

/** A Tag splitting row's line. */
export const splittingLine = (t: SplitText) => [`Artists: ${t.artistSeps || 'not split'}`, `Genres: ${t.genreSeps || 'not split'}`].join(' · ');
