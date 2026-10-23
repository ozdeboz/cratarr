// Review harness: a fake playing track (no server), shaped as music-core's Music.
import type { Music } from './lib/visuals/music-core';
import { TRACK_SIGNALS, BAND_SIGNALS } from './lib/visuals/routes';
export { musicOf, newMusicState, type Music, type MusicState, type Steps } from './lib/visuals/music-core';
const track = new Float32Array(TRACK_SIGNALS.length);
const bands = Array.from({ length: 6 }, () => new Float32Array(BAND_SIGNALS.length));
const wave = new Float32Array(1024);
export function musicNow(at: number): Music {
  const t = at / 1000;
  const bps = 2;
  const pos = t;
  const idx = Math.floor(pos * bps);
  const since = pos - idx / bps;
  track.fill(0.5);
  track[0] = 0.7 + 0.2 * Math.sin(t);
  for (let b = 0; b < 6; b++) for (let s = 0; s < BAND_SIGNALS.length; s++) bands[b][s] = 1 + 0.5 * Math.sin(t * (1 + b) + s);
  for (let i = 0; i < wave.length; i++) wave[i] = 0.6 * Math.sin(i / 20 + t * 5) * Math.sin(i / 300);
  return { key: 'local/review', intensity: 0.75, bps, beat: { index: idx, since, length: 1 / bps, strength: 0.8 }, track, bands, hits: null, strikes: null, from: 'live', playing: true, position: pos, typical: null, wave };
}
export function stopMusic() {}
