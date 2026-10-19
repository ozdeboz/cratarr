// The sections of a track's signals, by the server's ids (analysis/signals.rs): apart from signals.ts, which fetches
// them, so what only reads them (normal.ts) runs in Node's tests too.

/** The sections, by the server's ids (analysis/signals.rs). */
export const ID = {
  table: 0,
  loudness: 1,
  level: 2,
  flux: 3,
  hits: 4,
  percussive: 5,
  harmonic: 6,
  percussiveHits: 7,
  crest: 8,
  width: 9,
  centroid: 10,
  chroma: 11,
  harmonicChange: 12,
  harmonicChanges: 13,
  novelty: 14,
  boundaries: 15,
  bar: 16,
  swell: 17,
  bandSwell: 18,
  buildUp: 19,
} as const;
