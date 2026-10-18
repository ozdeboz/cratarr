// The rating page's pairs (decision 0046, amended): what the owner compares, two behaviours at a time on the music
// that plays, for the governor's calibration. Four kinds of pair, in turn, so the ratings say both which behaviours
// the owner likes and how far along each axis (Flash, Speed, the fluid, Intensity, how long lights ring) they like it:
//
// 0. a random recipe against Standard (production as it is);
// 1. one recipe at the low and at the high end of one axis;
// 2. two random recipes;
// 3. Standard at the low and at the high end of one axis.
//
// Which of a pair is shown first is drawn too (A is always the first shown). Pure: the n-th pair of a series is the same
// each time, so a series can be taken up again where it was left.

import { recipeFor, settingsOf } from './recipe';
import type { VisualSettings } from './settings.svelte';

export type Axis = 'flash' | 'speed' | 'fluid' | 'intensity' | 'lighting';
/** Each axis's low and high end: an absolute setting, or (lighting) a factor on the bands' times. */
export const AXES: Record<Axis, [number, number]> = {
  flash: [0.4, 1.6],
  speed: [0.5, 1.6],
  fluid: [0, 0.6],
  intensity: [0.4, 1],
  lighting: [0.5, 2],
};

export type Shown = { kind: 'standard' } | { kind: 'recipe'; seed: number } | { kind: 'variant'; of: 'standard' | number; axis: Axis; end: 'low' | 'high' };
export type Pair = { n: number; kind: 0 | 1 | 2 | 3; a: Shown; b: Shown; swapped: boolean };

/** A number of `x` and `n` (a series' base and the pair's place), spread over 32 bits. */
function mix(x: number, n: number): number {
  let h = (x ^ Math.imul(n + 1, 0x9e3779b1)) >>> 0;
  h = Math.imul(h ^ (h >>> 16), 0x85ebca6b) >>> 0;
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35) >>> 0;
  return (h ^ (h >>> 16)) >>> 0;
}

/** The `n`-th pair of the series `base`. */
export function pairFor(n: number, base: number): Pair {
  const h = mix(base, n);
  const kind = (n % 4) as Pair['kind'];
  const seed = mix(h, 1);
  const axes = Object.keys(AXES) as Axis[];
  // The axes in turn (every second pair has one): each comes up within ten pairs.
  const axis = axes[Math.floor(n / 2) % axes.length];
  let a: Shown;
  let b: Shown;
  if (kind === 0) [a, b] = [{ kind: 'recipe', seed }, { kind: 'standard' }];
  else if (kind === 1) [a, b] = [{ kind: 'variant', of: seed, axis, end: 'low' }, { kind: 'variant', of: seed, axis, end: 'high' }];
  else if (kind === 2) [a, b] = [{ kind: 'recipe', seed }, { kind: 'recipe', seed: mix(h, 3) }];
  else [a, b] = [{ kind: 'variant', of: 'standard', axis, end: 'low' }, { kind: 'variant', of: 'standard', axis, end: 'high' }];
  const swapped = (mix(h, 4) & 1) === 1;
  return swapped ? { n, kind, a: b, b: a, swapped } : { n, kind, a, b, swapped };
}

/** The settings `shown` stands for: on `standard` (production's behaviour on the device's own look and settings), a
 *  recipe for `aspects` (routes.ts's ASPECTS). */
export function settingsFor(shown: Shown, standard: VisualSettings, aspects: readonly string[]): VisualSettings {
  if (shown.kind === 'standard') return standard;
  if (shown.kind === 'recipe') return settingsOf(recipeFor(shown.seed, aspects, standard.lighting.length), standard);
  const on = shown.of === 'standard' ? standard : settingsOf(recipeFor(shown.of, aspects, standard.lighting.length), standard);
  const v = AXES[shown.axis][shown.end === 'low' ? 0 : 1];
  if (shown.axis === 'lighting') return { ...on, lighting: on.lighting.map((t) => Math.min(1, Math.max(0.02, t * v))) };
  return { ...on, [shown.axis]: v };
}

/** What a rating says of one of its pair: what it was (its recipe's extras, where it is one) and its meters. */
export function described(shown: Shown, aspects: readonly string[]): Record<string, unknown> {
  const seed = shown.kind === 'recipe' ? shown.seed : shown.kind === 'variant' && shown.of !== 'standard' ? shown.of : null;
  return { ...shown, extras: seed === null ? [] : recipeFor(seed, aspects).extras };
}
