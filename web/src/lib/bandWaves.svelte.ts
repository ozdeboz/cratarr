// A track's waveform in three bands (the server's analysis::band_wave): 512 bins each of low (below 150 Hz), mid and
// high (above 2.5 kHz), each band's RMS as 0..255 of the loudest bin of any band, so the bands keep their proportions.
// Read once per track (the server works it out the first time, a second or two) and kept here while the page is open.
// Undefined while it loads, null where the track has none (its server is not Cratarr's own, or it cannot be decoded).

import { asksSignIn, toSignIn } from './reach.svelte';

export type BandWave = { low: Uint8Array; mid: Uint8Array; high: Uint8Array };
/** How the full-screen player draws them (lib/BandGlass; Settings › Display › Player). */
export type BandLook = 'aurora' | 'sirius' | 'hairlines';

const BINS = 512;
const FORMAT = 1;
const waves = $state<Record<string, BandWave | null>>({});
const asked = new Set<string>();

/** Reads a track's bands, once. */
export function loadBandWave(source: string, id: string) {
  const key = `${source}/${id}`;
  if (asked.has(key)) return;
  asked.add(key);
  fetch(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/bands`)
    .then((r) => {
      if (asksSignIn(r)) toSignIn();
      return r.ok ? r.arrayBuffer() : null;
    })
    .then(
      (b) => {
        const v = b ? new Uint8Array(b) : null;
        waves[key] = v && v.length === 1 + 3 * BINS && v[0] === FORMAT ? { low: v.subarray(1, 1 + BINS), mid: v.subarray(1 + BINS, 1 + 2 * BINS), high: v.subarray(1 + 2 * BINS) } : null;
      },
      () => {
        // Asked again the next time it shows.
        asked.delete(key);
      },
    );
}

export const bandWaveOf = (source: string, id: string): BandWave | null | undefined => waves[`${source}/${id}`];
