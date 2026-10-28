// A track's waveform (decision 0035): its analysis' 512 bins, each its RMS (0..=255 of the track's loudest bin), read
// once per track (the browser keeps it a month, with an ETag) and kept here while the page is open. Undefined while it
// loads, null where the track has none (not analysed yet, or its server is not Cratarr's own).

import { AGAIN_MS, BUSY, TRIES, WAVE, askAnalysis } from './analysisAsks';
import { asksSignIn, toSignIn } from './reach.svelte';
import { BANDCAMP } from './bandcampPreview';
import { binFrames, frames, scaled } from './mp3Frames';

const waves = $state<Record<string, Uint8Array | null>>({});
const asked = new Set<string>();
/** How far a waveform being read has come, 0..1 (the owner, 2026-10-26: its line gets its gaps as it loads, a
 *  progress bar of its own); gone once it is in. */
const loading = $state<Record<string, number>>({});
export const waveLoading = (source: string, id: string): number | null => loading[`${source}/${id}`] ?? null;

/** A response's bytes, `onpart` told how far they have come (by its length, where it says one). */
async function readAll(r: Response, onpart: (share: number) => void): Promise<Uint8Array> {
  const total = Number(r.headers.get('content-length')) || 0;
  if (!r.body || !total) return new Uint8Array(await r.arrayBuffer());
  const reader = r.body.getReader();
  const parts: Uint8Array[] = [];
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
    got += value.length;
    onpart(Math.min(1, got / total));
  }
  const all = new Uint8Array(got);
  let at = 0;
  for (const p of parts) {
    all.set(p, at);
    at += p.length;
  }
  return all;
}

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
  loading[key] = 0;
  const r = await fetch(url);
  if (asksSignIn(r)) toSignIn();
  if (!r.ok) return null;
  // The file the first half of the way, its slices decoded the second.
  const bytes = await readAll(r, (share) => (loading[key] = share / 2));
  const mp3 = frames(bytes);
  if (!mp3) return null;
  heard[key] = { format: 'MP3', sample_rate_hz: mp3.sampleRate, channels: mp3.channels, bitrate_kbps: mp3.bitrateKbps };
  if (typeof OfflineAudioContext === 'undefined') return null;
  const ctx = new OfflineAudioContext(1, 1, mp3.sampleRate);
  const bins = binFrames(mp3.starts.length, BINS);
  const rms: number[] = [];
  for (let i = 0; i < bins.length; i += SLICE) {
    loading[key] = 0.5 + i / bins.length / 2;
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

/** How often each track's waveform found the server busy; the server busy, already set to be asked again. */
const busyTries = new Map<string, number>();
const BUSY_AGAIN = new Error('busy');

/** Reads a track's waveform, once; a Bandcamp preview's is worked out here. The server busy, asked again a little later
 *  (lib/analysisAsks), not taken as none. */
export function loadWaveform(source: string, id: string) {
  const key = `${source}/${id}`;
  if (!asked.has(key) && source === BANDCAMP) {
    asked.add(key);
    analyse(key, `/api/file/${encodeURIComponent(source)}/${encodeURIComponent(id)}`)
      .then(
        (w) => (waves[key] = w),
        () => (waves[key] = null),
      )
      .finally(() => delete loading[key]);
  }
  if (!asked.has(key)) {
    asked.add(key);
    loading[key] = 0;
    askAnalysis(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/waveform`, WAVE)
      .then((r) => {
        if (asksSignIn(r)) toSignIn();
        const tries = (busyTries.get(key) ?? 0) + 1;
        if (r.status === BUSY && tries < TRIES) {
          busyTries.set(key, tries);
          setTimeout(() => {
            asked.delete(key);
            loadWaveform(source, id);
          }, AGAIN_MS);
          throw BUSY_AGAIN;
        }
        return r.ok ? readAll(r, (share) => (loading[key] = share)) : null;
      })
      .then(
        (b) => (waves[key] = b && b.byteLength ? b : null),
        (e) => {
          // Asked again the next time it shows, not before AGAIN_MS (it showed four times a second, the page unreachable).
          if (e !== BUSY_AGAIN) setTimeout(() => asked.delete(key), AGAIN_MS);
        },
      )
      .finally(() => delete loading[key]);
  }
}

export const waveformOf = (source: string, id: string): Uint8Array | null | undefined => waves[`${source}/${id}`];

/** The bins as `n` bars, 0..1 each: the loudest bin of each stretch, against the track's loudest (its shape, however
 *  loud it was mastered). */
/** The bars of the stretch `from`..`to` (0..1 of the track) on `n` pieces: a zoom remapped onto the same pieces (the
 *  owner, 2026-10-28). Finer than the bins: between the two nearest. Scaled as the whole track's are (its loudest 1),
 *  and by `contrast` (0..1) toward the stretch's own range, its quietest at the floor and its loudest at the ceiling,
 *  so where the playhead is reads at a glance. */
export function barsIn(bins: Uint8Array, n: number, from: number, to: number, contrast = 0): number[] {
  const plain = barsPlainIn(bins, n, from, to);
  if (contrast <= 0) return plain;
  const lo = Math.min(...plain);
  const hi = Math.max(...plain);
  if (hi - lo < 1e-3) return plain;
  return plain.map((v) => v + ((v - lo) / (hi - lo) - v) * contrast);
}
function barsPlainIn(bins: Uint8Array, n: number, from: number, to: number): number[] {
  let top = 1;
  for (let j = 0; j < bins.length; j++) top = Math.max(top, bins[j]);
  const L = bins.length;
  const per = ((to - from) * L) / n;
  return Array.from({ length: n }, (_, i) => {
    const a = from * L + i * per;
    if (per < 1) {
      const x = Math.min(L - 1, Math.max(0, a + per / 2 - 0.5));
      const j = Math.floor(x);
      const k = Math.min(L - 1, j + 1);
      return (bins[j] + (bins[k] - bins[j]) * (x - j)) / top;
    }
    let m = 0;
    for (let j = Math.max(0, Math.floor(a)); j < Math.min(L, Math.ceil(a + per)); j++) m = Math.max(m, bins[j]);
    return m / top;
  });
}
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
