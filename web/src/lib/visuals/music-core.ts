// The music's signals at a moment of a track whose signals the server has (decision 0046, amended): pure, so the
// visuals' loop (music.svelte.ts `musicNow`), the precheck and the engine's Music module (decision 0057) step it alike,
// and Node's tests too. The whole track's signals and each band's, as `Music` gives them, from signals.ts's sections and
// beats.ts's beats.
import { beatAt, tempoAt, type Beat, type Beats } from './beats';
import type { Strike } from './engine/modules';
import { followEnvelopes } from './envelope';
import { typicalOf, type Typical } from './normal';
import { B, BAND_SIGNALS, T, TRACK_SIGNALS } from './routes';
import { ID, lastOf, lufsAt, seriesAt, type Events, type Signals } from './signals';
import { BANDS, visualSettings } from './settings.svelte';

/** A band's last hit, for the steps the Continuity setting asks for: its place among the band's hits (-1 before the
 *  first), its strength against the band's mean (1: as strong as its hits are on average), and the band's hits a
 *  second; and how soon its next one comes (s), where that is known ahead (the server's; null as heard). */
export type Steps = { index: number; weight: number; perSecond: number; nextIn: number | null };

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
  /** Each band's strikes, its hits and its drum hits as triggers (an ADSR's input, decision 0057); null where none are
   *  known here (yet). */
  strikes: BandStrikes[] | null;
  /** Where the bands' signals come from. */
  from: 'server' | 'live' | null;
  playing: boolean;
  /** Where in the track the picture is drawn (seconds, a little ahead of the sound: `AHEAD`); 0 with none. */
  position: number;
  /** The track's typical values (normal.ts, decision 0056), from the server's signals and its beats where they are
   *  in; null with neither. */
  typical: Typical | null;
  /** The sound's last samples (−1..1), where this device plays it (live.ts), for the Waves source; null elsewhere. */
  wave: Float32Array | null;
};

export type BandStrikes = { hits: Strike; drums: Strike };
const noStrikes = (): BandStrikes => ({ hits: { since: Infinity, velocity: 0, next: null }, drums: { since: Infinity, velocity: 0, next: null } });

/** The loudness as heard that makes the show's intensity nothing, and all (LUFS): a quiet room's music to a loud
 *  master's chorus, in equal steps of dB. */
const QUIET = -45;
const LOUD = -10;

/** A section's start: a mark of its strength fading over this long (s). */
const SECTION_S = 1;
/** The key follows the chroma over this long (s): a chord is not a key. */
const KEY_S = 2;

/** What the music's signals keep from frame to frame (a frame's `Music` is read within its frame): the signals now,
 *  each band's last hit, and the key (the chroma's direction on the circle of fifths, followed as a vector, so it turns
 *  the short way round). */
export type MusicState = {
  track: Float32Array;
  bands: Float32Array[];
  hits: Steps[];
  strikes: BandStrikes[];
  fifths: { x: number; y: number; at: number };
  /** Each band's envelope's hold (what its sound still holds, followed), and when it was last followed (ms; −1 never). */
  held: Float32Array;
  heldAt: number;
};
export function newMusicState(): MusicState {
  return {
    track: new Float32Array(TRACK_SIGNALS.length),
    bands: Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length)),
    hits: Array.from({ length: BANDS }, () => ({ index: -1, weight: 1, perSecond: 0, nextIn: null })),
    strikes: Array.from({ length: BANDS }, noStrikes),
    fifths: { x: 0, y: 0, at: 0 },
    held: new Float32Array(BANDS),
    heldAt: -1,
  };
}

// The circle of fifths' place of each pitch class (C, C#, ... B), as an angle.
const FIFTH_ANGLE = Array.from({ length: 12 }, (_, k) => (2 * Math.PI * ((7 * k) % 12)) / 12);

/** A hit's light: as strong as the hit (a weak one weak), fading over `ring` seconds. */
export const ringOf = (since: number, strength: number, ring: number) => (since > ring * 5 ? 0 : strength * Math.exp(-since / ring));

/** A beat's pulse: on at the beat (most on the strong ones), gone within `most` seconds (sooner at a fast tempo). */
const pulseOf = (b: Beat, most: number, share: number) => (0.35 + 0.65 * b.strength) * Math.exp(-b.since / Math.min(most, share * b.length));

/** Where the hits and beats are known ahead (the server's), a light or pulse rises into each over this long (s) rather
 *  than coming on all at once, its most still on the moment: a light on in one frame and gone in the next was a strobe
 *  (2026-10-18). The sound heard as it plays cannot be got ahead of, so its hits still come on at once. */
export const ATTACK = 0.04;
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

/** A list of events' strike at `pos`, into `s`: the last one, and the next where it is within ATTACK. */
function strikeOf(s: Strike, e: Events | undefined, pos: number) {
  const last = lastOf(e, pos);
  const next = lastOf(e, pos + ATTACK);
  s.since = last.index < 0 ? Infinity : last.since;
  s.velocity = last.index < 0 ? 0 : last.value;
  s.next = next.index > last.index ? { until: ATTACK - next.since, velocity: next.value } : null;
}

/** A beat's pulse at `pos` (`beat`: the one at or before it), rising into the next where it is near; `counts`: whether
 *  a beat pulses at all (a downbeat's only those that start a bar), and how much. */
function pulseAt(beats: Beats, beat: Beat | null, pos: number, most: number, share: number, counts: (b: Beat) => number): number {
  const on = beat ? pulseOf(beat, most, share) * counts(beat) : 0;
  const next = beatAt(beats, pos + ATTACK);
  if (!next || (beat && next.index <= beat.index)) return on;
  return Math.max(on, (0.35 + 0.65 * next.strength) * counts(next) * risen(ATTACK - next.since));
}

/** The show's intensity from the loudness heard (LUFS, null where it is not known) at the player's gain (dB), else
 *  from the waveform's energy there (0..1, null where that is not known either). */
export function intensityOf(heard: number | null, gainDb: number, energy: number | null): number {
  return heard !== null ? Math.min(1, Math.max(0, (heard + gainDb - QUIET) / (LOUD - QUIET))) : 0.25 + 0.6 * (energy ?? 0.6);
}

/** The Tempo row follows the beat (decision 0049's interim; the owner, 2026-10-19: "now"): the track's tempo, faded by
 *  how strongly its last bar's beats fell, so it goes quiet where nothing beats. A beat grid runs on through a break at
 *  weak beats (a trip-hop break: 33 beats under 0.15, 0049), and a track without one has none at all (0). A bar's
 *  mean strength of PULSE_NONE or less gives none of it, PULSE_FULL or more all of it. PROVISIONAL, measured on 12
 *  tracks of the Test crate (2026-10-19): a steady bar's mean is 0.3 to 0.85 (its median per track 0.32 to 0.84), a
 *  break's under 0.15; at 0.35 the grooves are at all of it 69 to 95 % of the time, the sparse ones about half. The
 *  turning's own clock keeps the grid's tempo (aspects.ts, from `bps`), so the flame does
 *  not stand still in ambient passages. */
const BAR = 4;
const PULSE_NONE = 0.15;
const PULSE_FULL = 0.35;

/** The mean strength of the bar's beats ending at beat `i`. */
function barStrength(b: Beats, i: number): number {
  const from = Math.max(0, i - BAR + 1);
  let sum = 0;
  for (let k = from; k <= i; k++) sum += b.strength[k];
  return sum / (i - from + 1);
}

/** How much of the tempo the Tempo row gives now (0..1): from the bar before this beat to the bar ending at it, across
 *  the beat, so it never jumps; and where no next beat comes (a gap, the end), going out over the half beat before
 *  the beat is gone (beatAt). */
function pulseShare(beats: Beats | null, beat: Beat | null): number {
  if (!beats || !beat) return 0;
  const was = barStrength(beats, Math.max(0, beat.index - 1));
  const now = barStrength(beats, beat.index);
  const mean = was + (now - was) * (beat.length > 0 ? Math.min(1, beat.since / beat.length) : 1);
  const x = Math.min(1, Math.max(0, (mean - PULSE_NONE) / (PULSE_FULL - PULSE_NONE)));
  const out = beat.length > 0 ? Math.min(1, Math.max(0, (beat.since - beat.length) / (0.5 * beat.length))) : 0;
  return x * x * (3 - 2 * x) * (1 - out * out * (3 - 2 * out));
}

/** The whole track's signals every source gives: the loudness, the tempo, the beat, the key. */
export function finish(st: MusicState, intensity: number, bps: number, beats: Beats | null, beat: Beat | null, pos: number) {
  st.track[T.loudness] = intensity;
  st.track[T.tempo] = (bps / 2) * pulseShare(beats, beat);
  if (beats) st.track[T.beat] = pulseAt(beats, beat, pos, 0.2, 0.35, () => 1);
  st.track[T.key] = Math.atan2(st.fifths.y, st.fifths.x) / (2 * Math.PI) + (st.fifths.y < 0 ? 1 : 0);
}

/** The music at `position` (seconds) of a track whose signals the server has, playing or not: what the visuals' loop
 *  takes each frame (`musicNow`), and what the behaviour's precheck steps through a track with (recipe.ts) without it
 *  playing. `state`: what follows from frame to frame (a new one for each track stepped through; without one, a new one
 *  each call, so the key does not follow); `at`: the frame's clock (ms; by default the position's); `lighting`: each
 *  band's hits' ring (s; by default the settings'); `gainDb`: the player's levelling; `bpm`: the track's tempo where it
 *  has no beats; `energy`: the waveform's loudness there (0..1), where the signals' is not known. */
export function musicOf(
  signals: Signals,
  beats: Beats | null,
  position: number,
  playing: boolean,
  opts: { state?: MusicState; at?: number; key?: string | null; lighting?: readonly number[]; gainDb?: number; bpm?: number | null; energy?: number | null } = {},
): Music {
  const st = opts.state ?? newMusicState();
  st.track.fill(0);
  const beat = beats ? beatAt(beats, position) : null;
  const bps = beats ? 1 / tempoAt(beats, position) : (opts.bpm ?? 90) / 60;
  fromServer(st, signals, position, beats, beat, opts.at ?? position * 1000, opts.lighting ?? visualSettings.lighting);
  const intensity = intensityOf(lufsAt(signals, position), opts.gainDb ?? 0, opts.energy ?? null);
  finish(st, intensity, bps, beats, beat, position);
  const typical = typicalOf(signals, beats);
  return { key: opts.key ?? null, intensity, bps, beat, track: st.track, bands: st.bands, hits: st.hits, strikes: st.strikes, from: 'server', playing, position, typical, wave: null };
}

/** The server's signals at `pos`, into `st`. */
function fromServer(st: MusicState, s: Signals, pos: number, beats: Beats | null, beat: Beat | null, at: number, lighting: readonly number[]) {
  const { track, bands, hits, strikes, fifths } = st;
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
    st.fifths = { x: fifths.x + (x - fifths.x) * k, y: fifths.y + (y - fifths.y) * k, at };
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
    const ring = lighting[b];
    v[B.hits] = lightOf(all?.[b], pos, ring);
    v[B.percussiveHits] = lightOf(drums?.[b], pos, ring);
    strikeOf(strikes[b].hits, all?.[b], pos);
    strikeOf(strikes[b].drums, drums?.[b], pos);
    const hit = lastOf(all?.[b], pos);
    const next = all?.[b]?.at[hit.index + 1];
    hits[b] = { index: hit.index, weight: hit.value / Math.max(0.05, s.meanStrength[b] ?? 1), perSecond: s.perSecond[b] ?? 0, nextIn: next === undefined ? null : next - pos };
  }
  followEnvelopes(st, at, lighting);
}

