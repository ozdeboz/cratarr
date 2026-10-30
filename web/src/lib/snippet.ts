// A track's snippet (0.12, the owner's mockup A2): its liveliest SNIPPET_S seconds, heard from its menu and from the
// crate search's rows. Found in its waveform (decision 0035's bins, each its RMS): the window of the highest mean,
// leaving out the first and last EDGE of the track (an intro's or an outro's swell is not the track), its start on the
// nearest beat where the track's beats are known. A track under SHORT_S: the whole of it.

/** How long a snippet is (s). */
export const SNIPPET_S = 20;
/** The share of a track at each end a snippet does not start or end in. */
export const EDGE = 0.08;
/** Where a snippet starts in a track whose waveform is not stored (its share of the track): no energy to go by, and
 *  nothing worked out for it (2026-10-30: a menu never asks the server to work anything out). */
export const FIXED_AT = 0.35;
/** A track shorter than this (s) is its own snippet. */
export const SHORT_S = 30;
/** How far a start may move to a beat (s): further, it stays where the energy put it. */
export const SNAP_S = 1.5;
/** What plays here while a snippet sounds: its share of its volume. */
export const DUCK = 0.2;
/** A snippet's fade in and out, and the duck's way down and up (ms). */
export const FADE_MS = 300;
/** A menu opened and closed within this (ms) plays nothing: a quick ⋮ and away does not blip. */
export const START_DELAY_MS = 300;

export type Stretch = { from: number; to: number };

/** The liveliest SNIPPET_S of a track `duration` s long whose waveform is `bins` (null: none stored, then from FIXED_AT
 *  of it). */
export function pickSnippet(bins: ArrayLike<number> | null, duration: number): Stretch {
  if (!(duration > 0)) return { from: 0, to: 0 };
  if (duration < SHORT_S) return { from: 0, to: duration };
  const lo = duration * EDGE;
  const hi = duration * (1 - EDGE);
  const span = Math.min(SNIPPET_S, hi - lo);
  if (!bins || !bins.length) {
    const from = Math.min(duration * FIXED_AT, hi - span);
    return { from, to: from + span };
  }
  const n = bins.length;
  const per = duration / n;
  // The window in whole bins, each bin's energy once; the best start by its mean, the earliest on a tie.
  const w = Math.max(1, Math.round(span / per));
  const first = Math.ceil(lo / per);
  const last = Math.floor(hi / per) - w;
  if (last < first) return { from: lo, to: lo + span };
  let sum = 0;
  for (let i = first; i < first + w; i++) sum += bins[i];
  let best = sum;
  let at = first;
  for (let i = first + 1; i <= last; i++) {
    sum += bins[i + w - 1] - bins[i - 1];
    if (sum > best) {
      best = sum;
      at = i;
    }
  }
  const from = Math.max(lo, at * per);
  return { from, to: Math.min(hi, from + span) };
}

/** `s` moved to the nearest of `beats` (seconds, in order) within SNAP_S, kept so that a stretch `span` long from it
 *  stays inside 0..`duration`; no beats near: `s` as it is. */
export function snapToBeat(s: number, beats: ArrayLike<number> | null, duration: number, span: number): number {
  if (!beats || !beats.length) return s;
  let best = s;
  let gap = SNAP_S;
  for (let i = 0; i < beats.length; i++) {
    const d = Math.abs(beats[i] - s);
    if (d <= gap && beats[i] >= 0 && beats[i] + span <= duration) {
      gap = d;
      best = beats[i];
    }
    if (beats[i] > s + SNAP_S) break;
  }
  return best;
}

/** The track's snippet: the liveliest stretch, its start on a beat where its beats are known; with no waveform, from
 *  FIXED_AT, on no beat. */
export function snippetOf(bins: ArrayLike<number> | null, duration: number, beats: ArrayLike<number> | null = null): Stretch {
  const s = pickSnippet(bins, duration);
  if (duration < SHORT_S || !bins || !bins.length) return s;
  const span = s.to - s.from;
  const from = snapToBeat(s.from, beats, duration, span);
  return { from, to: from + span };
}

/** Eased from 0 to 1 (smoothstep), for the fades and the duck: no corner to click at either end. */
export const ease = (x: number) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));

/** The snippet's own volume `ms` into it, `length` ms long: in over FADE_MS, out over the last FADE_MS. */
export function snippetGain(ms: number, length: number): number {
  const fade = Math.min(FADE_MS, length / 2);
  return Math.min(ease(ms / fade), ease((length - ms) / fade));
}

/** What plays here, `ms` into a duck going down (`down`) or back up: 1 to DUCK over FADE_MS, or back. */
export function duckGain(ms: number, down: boolean, from = down ? 1 : DUCK): number {
  const to = down ? DUCK : 1;
  return from + (to - from) * ease(ms / FADE_MS);
}
