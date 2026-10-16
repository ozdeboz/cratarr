// A track's waveform (decision 0035): its analysis' 512 bins, each its RMS (0..=255 of the track's loudest bin), read
// once per track (the browser keeps it a month, with an ETag) and kept here while the page is open. Undefined while it
// loads, null where the track has none (not analysed yet, or its server is not Cratarr's own).

import { asksSignIn, toSignIn } from './reach.svelte';

const waves = $state<Record<string, Uint8Array | null>>({});
const asked = new Set<string>();

/** Reads a track's waveform, once. */
export function loadWaveform(source: string, id: string) {
  const key = `${source}/${id}`;
  if (!asked.has(key)) {
    asked.add(key);
    fetch(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/waveform`)
      .then((r) => {
        if (asksSignIn(r)) toSignIn();
        return r.ok ? r.arrayBuffer() : null;
      })
      .then(
        (b) => (waves[key] = b && b.byteLength ? new Uint8Array(b) : null),
        () => {
          // Asked again the next time it shows.
          asked.delete(key);
        },
      );
  }
}

export const waveformOf = (source: string, id: string): Uint8Array | null | undefined => waves[`${source}/${id}`];

/** The bins as `n` bars, 0..1 each: the loudest bin of each stretch, against the track's loudest (its shape, however
 *  loud it was mastered). */
export function bars(bins: Uint8Array, n: number): number[] {
  const top = Math.max(1, ...bins);
  return Array.from({ length: n }, (_, i) => {
    const from = Math.floor((i * bins.length) / n);
    const to = Math.max(from + 1, Math.floor(((i + 1) * bins.length) / n));
    let m = 0;
    for (let j = from; j < to; j++) m = Math.max(m, bins[j]);
    return m / top;
  });
}
