// What the visuals follow (decision 0046, amended): the playing track's signals now, each 0..1, in the routes' order
// (routes.ts): the whole track's (its loudness as heard, beats and downbeats, tempo, sections, harmonic change, timbre,
// key, swells and build-ups) and each of its six bands' (level, flux, hits, percussive and harmonic parts, punch, width,
// swells). From the track's signals, worked out on the server (signals.ts); until the server has them, and where it has
// none, from the sound as it plays where the page plays it (live.ts); in the app the music plays natively, out of the
// page's reach, and on another player it plays elsewhere: there the loudness comes from the waveform until the server's
// signals are in. Read once per frame by the visuals' loop.
import { getTrackGain, trackState, type TrackGain } from '../api';
import { appInfo, inApp } from '../app.svelte';
import { prefs } from '../prefs.svelte';
import { audioTap, positionNow } from '../localPlayer.svelte';
import { isLocal } from '../output.svelte';
import { paused, playing } from '../remote.svelte';
import { loadWaveform, waveformOf } from '../waveforms.svelte';
import { beatAt, beatsOf, type Beat, type Beats } from './beats';
import { startLive, type LiveNow } from './live';
import { B, BAND_SIGNALS, T, TRACK_SIGNALS } from './routes';
import { ID, lastOf, lufsAt, seriesAt, signalsOf, type Events, type Signals } from './signals';
import { BANDS, visualSettings } from './settings.svelte';

/** A band's last hit, for the steps the Continuity setting asks for: its place among the band's hits (-1 before the
 *  first), its strength against the band's mean (1: as strong as its hits are on average), and the band's hits a
 *  second. */
export type Steps = { index: number; weight: number; perSecond: number };

export type Music = {
  /** The playing track (`source/id`), or null. */
  key: string | null;
  /** The loudness as heard here, from near nothing at -45 LUFS (or silence) to all at -10 (0..1); from the waveform
   *  where it is not known (yet). */
  intensity: number;
  /** Beats a second: the beats' own where they are known, else the tempo's, else 90 a minute. */
  bps: number;
  /** The beat it is at, where it has beats (a tempo) and they are known here yet. */
  beat: Beat | null;
  /** The whole track's signals now, in `TRACK_SIGNALS` order (the loudness's is `intensity`). */
  track: Float32Array;
  /** Each band's signals now, in `BAND_SIGNALS` order; null where none are known here (yet). */
  bands: Float32Array[] | null;
  hits: Steps[] | null;
  /** Where the bands' signals come from. */
  from: 'server' | 'live' | null;
  playing: boolean;
};

/** The picture is drawn this far ahead of the sound (seconds): a frame reaches the screen a frame or two after it is
 *  drawn, and a flash a little early reads as on the beat, a little late as after it. The sound heard as it plays
 *  cannot be got ahead of. */
const AHEAD = 0.05;

const tempos = new Map<string, number | null>();

/** The loudness as heard that makes the show's intensity nothing, and all (LUFS): a quiet room's music to a loud
 *  master's chorus, in equal steps of dB. */
const QUIET = -45;
const LOUD = -10;

/** A section's start: a mark of its strength fading over this long (s). */
const SECTION_S = 1;
/** The key follows the chroma over this long (s): a chord is not a key. */
const KEY_S = 2;

/** The gain the player plays a track at, as the players level it (decision 0035: never louder than as it is, nor past
 *  its peak), in dB; 0 where it does not level, or the gain is not known yet. */
const gains = new Map<string, TrackGain | null>();
function levelDb(source: string, id: string): number {
  if (!(inApp ? appInfo.levelling : prefs.levelling)) return 0;
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

// Kept from frame to frame (the loop reads them within its frame).
const track = new Float32Array(TRACK_SIGNALS.length);
const bands = Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length));
const hits: Steps[] = Array.from({ length: BANDS }, () => ({ index: -1, weight: 1, perSecond: 0 }));
// The key: the chroma's direction on the circle of fifths, followed as a vector (so it turns the short way round).
let fifths = { x: 0, y: 0, at: 0 };
// The circle of fifths' place of each pitch class (C, C#, ... B), as an angle.
const FIFTH_ANGLE = Array.from({ length: 12 }, (_, k) => (2 * Math.PI * ((7 * k) % 12)) / 12);

/** A hit's light: as strong as the hit (a weak one weak), fading over `ring` seconds. */
const ringOf = (since: number, strength: number, ring: number) => (since > ring * 5 ? 0 : strength * Math.exp(-since / ring));

/** A beat's pulse: on at the beat (most on the strong ones), gone within `most` seconds (sooner at a fast tempo). */
const pulseOf = (b: Beat, most: number, share: number) => (0.35 + 0.65 * b.strength) * Math.exp(-b.since / Math.min(most, share * b.length));

/** Where the hits and beats are known ahead (the server's), a light or pulse rises into each over this long (s) rather
 *  than coming on all at once, its most still on the moment: a light on in one frame and gone in the next was a strobe
 *  (2026-10-18). The sound heard as it plays cannot be got ahead of, so its hits still come on at once. */
const ATTACK = 0.04;
/** How far a light has risen with `until` seconds to go (0..1, smoothly). */
function risen(until: number): number {
  const x = 1 - until / ATTACK;
  return x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x);
}

/** A list of events' light at `pos`: the last one's fading over `ring` seconds, and the next one's rising. */
function lightOf(e: Events | undefined, pos: number, ring: number): number {
  const last = lastOf(e, pos);
  const next = lastOf(e, pos + ATTACK);
  const on = last.index < 0 ? 0 : ringOf(last.since, last.value, ring);
  return next.index > last.index ? Math.max(on, next.value * risen(ATTACK - next.since)) : on;
}

/** A beat's pulse at `pos` (`beat`: the one at or before it), rising into the next where it is near; `counts`: whether
 *  a beat pulses at all (a downbeat's only those that start a bar), and how much. */
function pulseAt(beats: Beats, beat: Beat | null, pos: number, most: number, share: number, counts: (b: Beat) => number): number {
  const on = beat ? pulseOf(beat, most, share) * counts(beat) : 0;
  const next = beatAt(beats, pos + ATTACK);
  if (!next || (beat && next.index <= beat.index)) return on;
  return Math.max(on, (0.35 + 0.65 * next.strength) * counts(next) * risen(ATTACK - next.since));
}

/** The music now. `at` is the frame's time (performance.now()), to move the position on between the player's reports. */
export function musicNow(at: number): Music {
  const np = playing();
  track.fill(0);
  if (!np) {
    stopLive();
    return { key: null, intensity: 0, bps: 1.5, beat: null, track, bands: null, hits: null, from: null, playing: false };
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
  const pos = positionAt(key, on, at) + (on ? AHEAD : 0);
  const beats = beatsOf(np.source, np.id);
  const signals = signalsOf(np.source, np.id);
  // The next track's too, so it pulses from its first beat.
  if (np.upcoming[0]) {
    beatsOf(np.source, np.upcoming[0].id);
    signalsOf(np.source, np.upcoming[0].id);
  }
  const beat = beats ? beatAt(beats, pos) : null;
  const bpm = tempos.get(key) ?? null;
  const bps = beat ? 1 / beat.length : (bpm ?? 90) / 60;

  // The bands: the server's, else heard here.
  let heard: number | null = null;
  let from: Music['from'] = null;
  if (signals) {
    stopLive();
    heard = lufsAt(signals, pos);
    fromServer(signals, pos, beats ?? null, beat, at);
    from = 'server';
  } else {
    const now = listen();
    if (now) {
      heard = now.loudness;
      fromLive(now, at);
      from = 'live';
    }
  }
  const wave = waveformOf(np.source, np.id);
  const length = np.progress?.duration_secs ?? null;
  const energy = wave && length ? loudnessAt(wave, pos / length) : null;
  const intensity =
    heard !== null ? Math.min(1, Math.max(0, (heard + levelDb(np.source, np.id) - QUIET) / (LOUD - QUIET))) : 0.25 + 0.6 * (energy ?? 0.6);

  track[T.loudness] = intensity;
  track[T.tempo] = bps / 2;
  if (beats) track[T.beat] = pulseAt(beats, beat, pos, 0.2, 0.35, () => 1);
  track[T.key] = Math.atan2(fifths.y, fifths.x) / (2 * Math.PI) + (fifths.y < 0 ? 1 : 0);
  return { key, intensity, bps, beat, track, bands: from ? bands : null, hits: from ? hits : null, from, playing: on };
}

/** The server's signals at `pos`. */
function fromServer(s: Signals, pos: number, beats: Beats | null, beat: Beat | null, at: number) {
  const one = (id: number) => seriesAt(s, id, pos)?.[0] ?? 0;
  // A downbeat: a beat that starts its bar, as surely as the server is of where bars start.
  const bar = s.series.get(ID.bar);
  if (beats && bar) {
    const surely = Math.min(1, (2 * bar.scale) / 255);
    const starts = (b: Beat) => (Math.round((seriesAt(s, ID.bar, pos, b.index)?.[0] ?? 1) * 255) === 0 ? surely : 0);
    track[T.downbeat] = pulseAt(beats, beat, pos, 0.35, 0.6, starts);
  }
  track[T.section] = lightOf(s.marks.get(ID.boundaries), pos, SECTION_S);
  track[T.harmony] = one(ID.harmonicChange);
  track[T.centroid] = one(ID.centroid);
  track[T.swell] = one(ID.swell);
  track[T.buildUp] = one(ID.buildUp);
  const chroma = seriesAt(s, ID.chroma, pos);
  if (chroma) {
    let x = 0;
    let y = 0;
    for (let k = 0; k < 12; k++) {
      x += chroma[k] * Math.cos(FIFTH_ANGLE[k]);
      y += chroma[k] * Math.sin(FIFTH_ANGLE[k]);
    }
    const k = 1 - Math.exp(-Math.min(0.1, Math.max(0, (at - fifths.at) / 1000)) / KEY_S);
    fifths = { x: fifths.x + (x - fifths.x) * k, y: fifths.y + (y - fifths.y) * k, at };
  } else fifths.at = at;

  const rows = (id: number) => seriesAt(s, id, pos);
  const level = rows(ID.level);
  const flux = rows(ID.flux);
  const percussive = rows(ID.percussive);
  const harmonic = rows(ID.harmonic);
  const crest = rows(ID.crest);
  const width = rows(ID.width);
  const swell = rows(ID.bandSwell);
  const all = s.hits.get(ID.hits);
  const drums = s.hits.get(ID.percussiveHits);
  for (let b = 0; b < BANDS; b++) {
    const v = bands[b];
    v[B.level] = level?.[b] ?? 0;
    v[B.flux] = flux?.[b] ?? 0;
    v[B.percussive] = percussive?.[b] ?? 0;
    v[B.harmonic] = harmonic?.[b] ?? 0;
    v[B.crest] = crest?.[b] ?? 0;
    v[B.width] = width?.[b] ?? 0;
    v[B.bandSwell] = swell?.[b] ?? 0;
    const ring = visualSettings.lighting[b];
    v[B.hits] = lightOf(all?.[b], pos, ring);
    v[B.percussiveHits] = lightOf(drums?.[b], pos, ring);
    const hit = lastOf(all?.[b], pos);
    hits[b] = { index: hit.index, weight: hit.value / Math.max(0.05, s.meanStrength[b] ?? 1), perSecond: s.perSecond[b] ?? 0 };
  }
}

// The sound heard as it plays: listened to while the visuals ask and the page's own player plays (on the tap it has
// while it plays); each band's hits as they arrive.
let live: { now(): LiveNow; stop(): void } | null = null;
let liveNode: AudioNode | null = null;
const heardHits = Array.from({ length: BANDS }, () => ({ count: 0, at: 0, strength: 0, mean: 0.6, since: 0, first: 0 }));

/** What has been heard lately, where the page plays the music; null elsewhere. */
function listen(): LiveNow | null {
  const tap = isLocal() ? audioTap() : null;
  if (!tap) {
    stopLive();
    return null;
  }
  if (!live || liveNode !== tap.node) {
    stopLive();
    live = startLive(tap.node, tap.context);
    liveNode = tap.node;
    const now = performance.now();
    for (const h of heardHits) Object.assign(h, { count: 0, at: 0, strength: 0, since: now, first: 0 });
  }
  return live.now();
}

function fromLive(now: LiveNow, at: number) {
  for (let b = 0; b < BANDS; b++) {
    const heard = now.bands[b];
    const v = bands[b];
    v.fill(0);
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
    v[B.hits] = h.count ? ringOf((at - h.at) / 1000, h.strength, visualSettings.lighting[b]) : 0;
    const secs = (at - h.since) / 1000;
    hits[b] = { index: h.count - 1, weight: h.strength / Math.max(0.05, h.mean), perSecond: secs > 5 ? (h.count - h.first) / secs : 2 };
  }
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

// The position at the frame. This device's player (the app's, or the page's own audio) says it to the frame. Another
// player reports it now and then, through the server: between reports it moves on with the frame clock, and it is
// eased toward each report rather than set to it, since a jump back or forth would be a beat played twice or skipped;
// a report far off (a seek) is taken as it is.
let clock = { key: '', pos: 0, at: 0, heard: Number.NaN };
function positionAt(key: string, on: boolean, at: number): number {
  if (isLocal()) return positionNow();
  const heard = playing()?.progress?.position_secs ?? 0;
  if (clock.key !== key) clock = { key, pos: heard, at, heard };
  if (on) clock.pos += (at - clock.at) / 1000;
  clock.at = at;
  if (heard !== clock.heard) {
    const off = heard - clock.pos;
    clock.pos += Math.abs(off) > 0.3 ? off : off / 2;
    clock.heard = heard;
  }
  return clock.pos;
}

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
