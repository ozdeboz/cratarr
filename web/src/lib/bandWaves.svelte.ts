// A track's spectrum (the server's analysis::spectrum): 32 bands from 25 Hz to 16 kHz, 20 frames a second, each a byte
// in decibels against the track's loudest (0: 60 dB or more below it). What the full-screen player's bands draw, in as
// many bands as the user chooses (3 to 32; lib/BandGlass). Read once per track (the server works it out the first time
// it plays, a few seconds) and kept here while the page is open. Undefined while it loads, null where the track has
// none (its server is not Cratarr's own, or it cannot be decoded).

import { AGAIN_MS, BUSY, PLAYING, TRIES, askAnalysis } from './analysisAsks';
import type { Prefs } from './prefs.svelte';
import { asksSignIn, toSignIn } from './reach.svelte';

export type Spectrum = {
  /** Measured bands, lowest first. */
  bands: number;
  /** Frames a second. */
  fps: number;
  frames: number;
  /** `frames` × `bands` bytes, a frame after another. */
  data: Uint8Array;
};
/** How the full-screen player draws them (lib/BandGlass; Settings › Display › Player visualisation). */
export type BandLook = 'aurora' | 'sirius' | 'hairlines' | 'meter';

/** The choices, in Settings › Display › Player visualisation and the player's own sheet (lib/BandSheet). */
export const BAND_LOOKS: { value: Prefs['playerBands']; label: string }[] = [
  { value: 'off', label: 'Off' },
  { value: 'aurora', label: 'Aurora' },
  { value: 'sirius', label: 'Sirius' },
  { value: 'hairlines', label: 'Hairlines' },
  { value: 'meter', label: 'Meter' },
];
/** How this device draws them: SVG, the finest, or a canvas, the fastest. */
export const BAND_ENGINES: { value: 'svg' | 'canvas'; label: string }[] = [
  { value: 'svg', label: 'Quality' },
  { value: 'canvas', label: 'Speed' },
];
export const ENGINE_MORE =
  'How this device draws them. Quality: every effect as designed (a true glow, the light blended), for a fast device. Speed: the same looks drawn far more cheaply, smooth on a slower one.';
export const BAND_DIRECTIONS: { value: 'left' | 'right'; label: string }[] = [
  { value: 'left', label: 'To the left' },
  { value: 'right', label: 'To the right' },
];
export const BANDS_MORE =
  "The track's sound from low to high, under the buttons when the full-screen player is landscape (a tap on them opens these settings).\n\nAurora: curtains of light. Sirius: pairs of ribbons. Hairlines: fine glowing lines. Meter: a bar for each band, its peak falling back over it.";
export const WINDOW_MORE =
  'How many seconds of music the bands show: for the waves the width, now in the middle; for the meter how long each bar takes in, so short moves with every beat and long with the phrase.';
export const COUNT_MORE = 'How many bands the sound is drawn in, from 3 (low, mid and high) to all 32 measured.';

const HEAD = 7;
const FORMAT = 1;
const FLOOR_DB = 60;
/** The three bands' kept form (analysis::band_wave): a format byte, then 512 bins of low, of mid and of high. */
const WAVE_BINS = 512;
const WAVE_FORMAT = 1;
const spectra = $state<Record<string, Spectrum | null>>({});
const asked = new Set<string>();

/** The server busy (lib/analysisAsks): not none, asked again a little later. */
class Busy extends Error {}
const get = (source: string, id: string, what: string) =>
  askAnalysis(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/${what}`, PLAYING).then((r) => {
    if (asksSignIn(r)) toSignIn();
    if (r.status === BUSY) throw new Busy();
    return r.ok ? r.arrayBuffer() : null;
  });
/** How often each track's spectrum found the server busy. */
const busyTries = new Map<string, number>();

/** A spectrum's bytes as read, or null. */
function spectrumFrom(b: ArrayBuffer | null): Spectrum | null {
  const v = b ? new Uint8Array(b) : null;
  if (!v || v.length < HEAD || v[0] !== FORMAT) return null;
  const bands = v[1];
  const frames = v[3] | (v[4] << 8) | (v[5] << 16) | (v[6] << 24);
  return bands > 0 && v.length === HEAD + frames * bands ? { bands, fps: v[2], frames, data: v.subarray(HEAD) } : null;
}

/** The three bands (a server without the spectrum: one before 0.8.6) as a spectrum of three bands, 512 frames over
 *  the track: their amplitude against the loudest in the spectrum's decibels. */
function wavesAsSpectrum(b: ArrayBuffer | null, duration: number): Spectrum | null {
  const v = b ? new Uint8Array(b) : null;
  if (!v || v.length !== 1 + 3 * WAVE_BINS || v[0] !== WAVE_FORMAT || !(duration > 0)) return null;
  const data = new Uint8Array(WAVE_BINS * 3);
  for (let f = 0; f < WAVE_BINS; f++)
    for (let band = 0; band < 3; band++) {
      const a = v[1 + band * WAVE_BINS + f] / 255;
      data[f * 3 + band] = a > 0 ? Math.round(Math.max(0, Math.min(1, 1 + (20 * Math.log10(a)) / FLOOR_DB)) * 255) : 0;
    }
  return { bands: 3, fps: WAVE_BINS / duration, frames: WAVE_BINS, data };
}

/** Reads a track's spectrum, once; where the server has none (one before 0.8.6), its three bands instead, so the looks
 *  draw the music as before (`duration`: the track's, s). */
export function loadSpectrum(source: string, id: string, duration: number) {
  const key = `${source}/${id}`;
  if (asked.has(key)) return;
  asked.add(key);
  get(source, id, 'spectrum')
    .then(async (b) => spectrumFrom(b) ?? wavesAsSpectrum(await get(source, id, 'band-wave'), duration))
    .then(
      (s) => {
        spectra[key] = s;
      },
      (e) => {
        // Asked again the next time it shows after AGAIN_MS; the server busy, by itself then too.
        const tries = (busyTries.get(key) ?? 0) + 1;
        setTimeout(() => {
          asked.delete(key);
          if (e instanceof Busy && tries < TRIES) {
            busyTries.set(key, tries);
            loadSpectrum(source, id, duration);
          }
        }, AGAIN_MS);
      },
    );
}

/** Silence: what the chosen look draws while a track's spectrum loads, or where it has none (the owner, 2026-10-24:
 *  drawn with input 0 until the track is loaded). */
export const SILENT: Spectrum = { bands: 32, fps: 20, frames: 0, data: new Uint8Array(0) };

export const spectrumOf = (source: string, id: string): Spectrum | null | undefined => spectra[`${source}/${id}`];
