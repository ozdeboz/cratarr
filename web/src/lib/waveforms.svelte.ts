// A track's waveform (decision 0035): its analysis' 512 bins, each its RMS (0..=255 of the track's loudest bin), read
// once per track (the browser keeps it a month, with an ETag) and kept here while the page is open. Undefined while it
// loads, null where the track has none (not analysed yet, or its server is not Cratarr's own).

import { asksSignIn, toSignIn } from './reach.svelte';
import { BANDCAMP } from './bandcampPreview';
import { binFrames, frames, scaled } from './mp3Frames';

const waves = $state<Record<string, Uint8Array | null>>({});
const asked = new Set<string>();

/** What the device read of a track it worked out itself (a Bandcamp preview's): its stream as its frames say. */
export type Heard = { format: string; sample_rate_hz: number; channels: number; bitrate_kbps: number };
const heard = $state<Record<string, Heard>>({});
export const heardOf = (source: string, id: string): Heard | null => heard[`${source}/${id}`] ?? null;

const BINS = 512;
/** Bins decoded at once: a few seconds of sound in memory, never the whole track. */
const SLICE = 16;

/** A Bandcamp preview's waveform worked out on the device (the owner, 2026-10-25), as the server's analysis has it:
 *  its file read once, its frames found from their headers, decoded a slice of bins at a time, each bin's RMS. */
async function analyse(key: string, url: string) {
  const r = await fetch(url);
  if (asksSignIn(r)) toSignIn();
  if (!r.ok) return null;
  const bytes = new Uint8Array(await r.arrayBuffer());
  const mp3 = frames(bytes);
  if (!mp3) return null;
  heard[key] = { format: 'MP3', sample_rate_hz: mp3.sampleRate, channels: mp3.channels, bitrate_kbps: mp3.bitrateKbps };
  if (typeof OfflineAudioContext === 'undefined') return null;
  const ctx = new OfflineAudioContext(1, 1, mp3.sampleRate);
  const bins = binFrames(mp3.starts.length, BINS);
  const rms: number[] = [];
  for (let i = 0; i < bins.length; i += SLICE) {
    const part = bins.slice(i, i + SLICE);
    const from = part[0][0];
    const to = part[part.length - 1][1];
    const end = to < mp3.starts.length ? mp3.starts[to] : mp3.end;
    let sound: AudioBuffer | null = null;
    try {
      sound = await ctx.decodeAudioData(bytes.slice(mp3.starts[from], end).buffer);
    } catch {
      /* a slice that does not decode: silent bins */
    }
    // Each bin its share of the slice's samples, as its share of the frames.
    let at = 0;
    for (const [a, b] of part) {
      const n = sound ? Math.round(((b - a) / (to - from)) * sound.length) : 0;
      let sum = 0;
      for (let c = 0; sound && c < sound.numberOfChannels; c++) {
        const data = sound.getChannelData(c);
        for (let j = at; j < Math.min(at + n, data.length); j++) sum += data[j] * data[j];
      }
      rms.push(n && sound ? Math.sqrt(sum / (n * sound.numberOfChannels)) : 0);
      at += n;
    }
  }
  return scaled(rms);
}

/** Reads a track's waveform, once; a Bandcamp preview's is worked out here. */
export function loadWaveform(source: string, id: string) {
  const key = `${source}/${id}`;
  if (!asked.has(key) && source === BANDCAMP) {
    asked.add(key);
    analyse(key, `/api/file/${encodeURIComponent(source)}/${encodeURIComponent(id)}`).then(
      (w) => (waves[key] = w),
      () => (waves[key] = null),
    );
  }
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
