// What the visuals follow (decision 0046, amended): the playing track's signals now, each 0..1, in the routes' order
// (routes.ts): the whole track's (its loudness as heard, beats and downbeats, tempo, sections, harmonic change, timbre,
// key, swells and build-ups) and each of its six bands' (level, flux, hits, percussive and harmonic parts, punch, width,
// swells). From the track's signals, worked out on the server (signals.ts); until the server has them, and where it has
// none, from the sound as it plays where the page plays it (live.ts); in the app the music plays natively, out of the
// page's reach, and on another player it plays elsewhere: there the loudness comes from the waveform until the server's
// signals are in. Read once per frame by the visuals' loop.
import { NEXT } from '../analysisAsks';
import { port } from '../player/port.svelte';
import { getTrackGain, trackState, type TrackGain } from '../api';
import { inApp } from '../app.svelte';
import { levelOn } from '../level.svelte';
import { prefs } from '../prefs.svelte';
import { audioTap } from '../localPlayer.svelte';
import { paused, playing } from '../remote.svelte';
import { placeAt } from '../trackPlace.svelte';
import { loadWaveform, waveformOf } from '../waveforms.svelte';
import { beatAt, beatsOf, tempoAt } from './beats';
import { followEnvelopes } from './envelope';
import { startAppLive, startLive, type LiveNow } from './live';
import { finish, intensityOf, musicOf, newMusicState, ringOf, type Music } from './music-core';
import { typicalOf } from './normal';
import { B } from './routes';
import { signalsOf } from './signals';
import { BANDS, visualSettings } from './settings.svelte';

// The pure part (musicOf and what it needs) is music-core.ts's; its types and steps are this module's as before.
export { musicOf, newMusicState, type Music, type MusicState, type Steps } from './music-core';

/** The picture is drawn this far ahead of the sound (seconds): a frame reaches the screen a frame or two after it is
 *  drawn, and a flash a little early reads as on the beat, a little late as after it. The sound heard as it plays
 *  cannot be got ahead of. */
const AHEAD = 0.05;

const tempos = new Map<string, number | null>();

/** The gain the player plays a track at, as the players level it (decision 0035: never louder than as it is, nor past
 *  its peak), in dB; 0 where it does not level, or the gain is not known yet. */
const gains = new Map<string, TrackGain | null>();
function levelDb(source: string, id: string): number {
  if (!levelOn()) return 0;
  const key = `${source}/${id}`;
  if (!gains.has(key)) {
    gains.set(key, null);
    getTrackGain(source, id).then(
      (g) => gains.set(key, g),
      () => gains.delete(key),
    );
  }
  const g = gains.get(key);
  if (!g || g.trackGain === undefined) return 0;
  const peak = g.trackPeak && g.trackPeak > 0 ? -20 * Math.log10(g.trackPeak) : 0;
  return Math.min(g.trackGain, peak, 0);
}

// The visuals' own.
const own = newMusicState();
const { track, bands, hits, strikes } = own;
/** The music now. `at` is the frame's time (performance.now()), to move the position on between the player's reports;
 *  `lighting`: each band's hits' ring (s; by default the settings'); `wantWave`: a source draws the sound's waveform
 *  (the Waves, decision 0056), so it is listened to even where the server's signals are in. */
export function musicNow(at: number, lighting: readonly number[] = visualSettings.lighting, wantWave = false): Music {
  const np = playing();
  track.fill(0);
  if (!np) {
    stopLive();
    return { key: null, intensity: 0, bps: 1.5, beat: null, track, bands: null, hits: null, strikes: null, from: null, playing: false, position: 0, typical: null, wave: null };
  }
  const key = `${np.source}/${np.id}`;
  loadWaveform(np.source, np.id);
  if (!tempos.has(key)) {
    tempos.set(key, null);
    trackState(np.source, np.id).then(
      (s) => tempos.set(key, s.info?.bpm ?? null),
      () => tempos.delete(key),
    );
  }
  const on = !paused();
  const pos = placeAt(key, on, at) + (on ? AHEAD : 0);
  const beats = beatsOf(np.source, np.id);
  const signals = signalsOf(np.source, np.id);
  // The next track's too, so it pulses from its first beat.
  if (np.upcoming[0]) {
    beatsOf(np.source, np.upcoming[0].id, NEXT);
    signalsOf(np.source, np.upcoming[0].id, NEXT);
  }
  const bpm = tempos.get(key) ?? null;
  const wave = waveformOf(np.source, np.id);
  const length = np.progress?.duration_secs ?? null;
  const energy = () => (wave && length ? loudnessAt(wave, pos / length) : null);

  // The bands: the server's, else heard here. With the server's, heard here only while a source draws the waveform:
  // listening (the app's sound sent twenty times a second, its statistics kept) costs a tablet much of a frame.
  if (signals) {
    let wave: Float32Array | null = null;
    if (wantWave) wave = listen()?.wave ?? null;
    else stopLive();
    const m = musicOf(signals, beats ?? null, pos, on, { state: own, at, key, lighting, gainDb: levelDb(np.source, np.id), bpm, energy: energy() });
    m.wave = wave;
    return m;
  }
  const now = listen();
  const beat = beats ? beatAt(beats, pos) : null;
  const bps = beats ? 1 / tempoAt(beats, pos) : (bpm ?? 90) / 60;
  let heard: number | null = null;
  let from: Music['from'] = null;
  if (now) {
    heard = now.loudness;
    fromLive(now, at, lighting);
    from = 'live';
  }
  const intensity = intensityOf(heard, levelDb(np.source, np.id), energy());
  finish(own, intensity, bps, beats ?? null, beat, pos);
  const typical = beats ? typicalOf(null, beats) : null;
  return { key, intensity, bps, beat, track, bands: from ? bands : null, hits: from ? hits : null, strikes: from ? strikes : null, from, playing: on, position: pos, typical, wave: now?.wave ?? null };
}

// The sound heard as it plays: listened to while the visuals ask and the page's own player plays (on the tap it has
// while it plays); each band's hits as they arrive.
let live: { now(): LiveNow; stop(): void } | null = null;
let liveNode: AudioNode | null = null;
const heardHits = Array.from({ length: BANDS }, () => ({ count: 0, at: 0, strength: 0, mean: 0.6, since: 0, first: 0 }));

/** In the app, its player's sound (the app hears it: startAppLive), as the node it is known by. */
const APP_EAR = {} as AudioNode;

/** What has been heard lately, where the page plays the music (this browser, or the app it runs in, which hears its own
 *  player); null elsewhere. */
function listen(): LiveNow | null {
  const tap = inApp ? (prefs.output === 'local' ? { node: APP_EAR, context: null } : null) : port().tap();
  if (!tap) {
    stopLive();
    return null;
  }
  if (!live || liveNode !== tap.node) {
    stopLive();
    live = tap.context ? startLive(tap.node, tap.context) : startAppLive();
    liveNode = tap.node;
    const now = performance.now();
    for (const h of heardHits) Object.assign(h, { count: 0, at: 0, strength: 0, since: now, first: 0 });
  }
  return live.now();
}

function fromLive(now: LiveNow, at: number, lighting: readonly number[]) {
  for (let b = 0; b < BANDS; b++) {
    const heard = now.bands[b];
    const v = bands[b];
    v.fill(0);
    for (const s of [strikes[b].hits, strikes[b].drums]) Object.assign(s, { since: Infinity, velocity: 0, next: null });
    if (!heard) continue;
    const h = heardHits[b];
    if (heard.hits !== h.count) {
      if (!h.first) h.first = heard.hits;
      h.count = heard.hits;
      h.at = at;
      h.strength = heard.lastHitStrength;
      h.mean += (heard.lastHitStrength - h.mean) * 0.05;
    }
    v[B.level] = heard.level;
    v[B.flux] = heard.flux;
    v[B.hits] = h.count ? ringOf((at - h.at) / 1000, h.strength, lighting[b]) : 0;
    Object.assign(strikes[b].hits, { since: h.count ? (at - h.at) / 1000 : Infinity, velocity: h.count ? h.strength : 0, next: null });
    const secs = (at - h.since) / 1000;
    hits[b] = { index: h.count - 1, weight: h.strength / Math.max(0.05, h.mean), perSecond: secs > 5 ? (h.count - h.first) / secs : 2, nextIn: null };
  }
  followEnvelopes(own, at, lighting);
}

function stopLive() {
  live?.stop();
  live = null;
  liveNode = null;
}

/** Lets go of the sound (the visuals closed). */
export function stopMusic() {
  stopLive();
}

// The position at the frame: lib/trackPlace's, the one clock the player's band glass draws by too.

/** The waveform's loudness at `fraction` of the track, between its bins, against its loudest bin. */
function loudnessAt(bins: Uint8Array, fraction: number): number {
  let top = 1;
  for (const b of bins) if (b > top) top = b;
  const x = Math.min(Math.max(fraction, 0), 1) * (bins.length - 1);
  const i = Math.floor(x);
  const a = bins[i];
  const b = bins[Math.min(i + 1, bins.length - 1)];
  return (a + (b - a) * (x - i)) / top;
}
