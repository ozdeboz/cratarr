// Which file of a track the tape head reads (decision 0102), as the app's `headKey` (Tracks.kt). Play from names a copy, and a
// copy is another file of the same track: what the head holds for one must not answer for the other, and the file is asked
// of the copy named, as every stream is.

type OfATrack = { source: string; id: string; from?: string };

/** What the head knows a track's file by: its source and id, then `@` and the copy where one is named. */
export const headKey = (t: OfATrack) => `${t.source}/${t.id}${t.from ? `@${t.from}` : ''}`;

/** That file's address. A library's id holds `/`, which must arrive whole. */
export const headFile = (t: OfATrack) =>
  `/api/file/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}${t.from ? `?from=${encodeURIComponent(t.from)}` : ''}`;
