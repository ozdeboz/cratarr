// The Clock's trigger outputs (decision 0057 §4b): Beat, Bar and Phrase from the Music's beats, each through a clock
// divider/multiplier (÷4 … ×4, the ratio knob of VCV Rack's clock modules, Impromptu's Clocked among them). Pure: a
// state and a step a frame, so the engine (graph.ts) wires it and the tests step it over recorded tracks.
//
// - A beat is a new beat index (Music.beat); the first frame seen gives none, as graph.ts's `onBeat` has it.
// - A bar is a beat that starts its bar: the downbeat signal on at that beat (music-core.ts puts it there only for a
//   beat the server's bar series starts a bar with). A track without a bar series has no bars, so no phrases.
// - A phrase is `phrase` bars (4; 8 where the declaration says so), the first bar seen starting one.
// - A divider (÷N) passes every Nth tick, the first of them included, counted from the level above: a beat's from its
//   bar, a bar's from its phrase, so ÷2 on beats gives the bar's first and third beats.
// - A multiplier (×N) adds N − 1 ticks between, where its period's place in beats crosses k/N of it: the beat's own
//   length and how long since it (Music.beat), so each beat sets them anew and a tempo change cannot make them drift;
//   a bar's and a phrase's in beats, as many as the last whole bar had.

import type { Music } from '../music-core';
import type { Knob } from '../patch';
import { T } from '../routes';

/** The ratios a clock output's knob steps through: ÷4, ÷3, ÷2, ×1, ×2, ×3, ×4. */
export const RATIOS = [1 / 4, 1 / 3, 1 / 2, 1, 2, 3, 4] as const;
export const RATIO_LABELS = ['÷4', '÷3', '÷2', '×1', '×2', '×3', '×4'] as const;
/** The step of ×1. */
export const UNIT_RATIO = 3;
/** A ratio knob's ratio (its step rounded; ×1 where it is not a number). */
export const ratioOf = (step: number | undefined) => (typeof step === 'number' && Number.isFinite(step) ? RATIOS[Math.min(RATIOS.length - 1, Math.max(0, Math.round(step)))] : 1);

/** A beat's downbeat signal above this at its beat: it starts a bar (music-core.ts's pulse is a third or more there). */
const DOWNBEAT = 0.05;
/** The most beats a bar is taken to have. */
const MOST_BEATS = 16;

/** The Clock's trigger knobs, as ports.ts's CLOCK would declare them (it has none yet): a ratio a trigger output, and
 *  the bars a phrase. Choices of how, not values: Randomize leaves them. */
export const CLOCK_TRIGGER_KNOBS: Knob[] = [
  ...(['beat', 'bar', 'phrase'] as const).map(
    (id): Knob => ({ id: `${id}Ratio`, label: `${id[0].toUpperCase()}${id.slice(1)} ÷×`, min: 0, max: RATIOS.length - 1, neutral: UNIT_RATIO, default: UNIT_RATIO, kind: 'step', randomize: false, options: RATIOS.map((r) => (r < 1 ? `÷${Math.round(1 / r)}` : `×${r}`)) }),
  ),
  { id: 'phraseBars', label: 'Bars a phrase', min: 4, max: 8, neutral: 4, default: 4, kind: 'step', randomize: false },
];

/** The knobs by those ids: each output's ratio step (RATIOS; ×1 unset), and the bars a phrase (4 unset). */
export type ClockTriggerKnobs = { beatRatio?: number; barRatio?: number; phraseRatio?: number; phraseBars?: number };

export type ClockTriggers = { beat: number; bar: number; phrase: number };
type Out = keyof ClockTriggers;
const OUTS: readonly Out[] = ['beat', 'bar', 'phrase'];

export type ClockTriggerState = {
  /** The beat index last seen (undefined: no frame yet; null: between beats). */
  beat: number | null | undefined;
  /** Beats since the bar began (0 on its downbeat; −1 before the first bar). */
  beats: number;
  /** How many beats the last whole bar had (4 until one is counted). */
  barBeats: number;
  /** Bars since the phrase began (0 on its first; −1 before the first bar). */
  bars: number;
  /** Each output's ticks counted for its divider, since the level above last ticked. */
  counts: Record<Out, number>;
  /** Each output's multiplier: the last k of its N it gave in its period. */
  sub: Record<Out, number>;
};

export const newClockTriggers = (): ClockTriggerState => ({ beat: undefined, beats: -1, barBeats: 4, bars: -1, counts: { beat: 0, bar: 0, phrase: 0 }, sub: { beat: 0, bar: 0, phrase: 0 } });

/** One frame: each output 1 on the frame it fires, else 0. `dt` is not needed (the beats carry the time) and kept for
 *  the engine's stepper shape. */
export function stepClockTriggers(st: ClockTriggerState, m: Music, knobs: ClockTriggerKnobs, _dt: number): ClockTriggers {
  const b = m.beat;
  const index = b?.index ?? null;
  const first = st.beat === undefined;
  const ticked = index !== null && !first && index !== st.beat;
  st.beat = index;
  const phraseBars = Math.max(1, Math.round(Number.isFinite(knobs.phraseBars) ? knobs.phraseBars! : 4));

  // The ticks at ×1: a beat, a bar on its downbeat, a phrase on every phraseBars-th bar.
  const tick: ClockTriggers = { beat: 0, bar: 0, phrase: 0 };
  if (ticked) {
    tick.beat = 1;
    if ((m.track[T.downbeat] ?? 0) > DOWNBEAT) {
      if (st.beats >= 0) st.barBeats = Math.min(MOST_BEATS, st.beats + 1);
      st.beats = 0;
      tick.bar = 1;
      st.bars = st.bars < 0 || st.bars + 1 >= phraseBars ? 0 : st.bars + 1;
      if (st.bars === 0) tick.phrase = 1;
    } else if (st.beats >= 0) st.beats = Math.min(st.beats + 1, MOST_BEATS * 8);
  }
  // A divider counts from the level above: a beat's from its bar, a bar's from its phrase.
  if (tick.bar) st.counts.beat = 0;
  if (tick.phrase) st.counts.bar = 0;

  // Where each period stands now, in beats, and how long it is (null: not known yet).
  const into = b && b.length > 0 ? Math.min(1, Math.max(0, b.since / b.length)) : null;
  const place: Record<Out, [number, number] | null> = {
    beat: into === null ? null : [into, 1],
    bar: into === null || st.beats < 0 ? null : [st.beats + into, st.barBeats],
    phrase: into === null || st.bars < 0 ? null : [st.bars * st.barBeats + st.beats + into, phraseBars * st.barBeats],
  };

  const out: ClockTriggers = { beat: 0, bar: 0, phrase: 0 };
  for (const o of OUTS) {
    const ratio = ratioOf(knobs[`${o}Ratio`]);
    if (tick[o]) {
      st.sub[o] = 0;
      if (ratio >= 1) out[o] = 1;
      else {
        const n = Math.round(1 / ratio);
        if (st.counts[o] % n === 0) out[o] = 1;
        st.counts[o] = (st.counts[o] + 1) % n;
      }
      continue;
    }
    const at = place[o];
    if (ratio <= 1 || !at || !(at[1] > 0)) continue;
    const k = Math.min(ratio - 1, Math.floor((at[0] / at[1]) * ratio + 1e-9));
    if (k > st.sub[o]) {
      st.sub[o] = k;
      // The first frame seen only finds its place, as it gives no beat.
      if (!first) out[o] = 1;
    }
  }
  return out;
}
