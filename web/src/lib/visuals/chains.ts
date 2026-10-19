// The visuals' preset chains (decision 0056: "chains are presets"). The owner, after 70: "we should also have some
// sensible presets, but nothing extreme the way we now have. Currently we have way too many lines. So we need a few
// sensible chains as presets, the ability to start from scratch"; asked which: Calm, Pulse and Flow; and from scratch,
// nothing at all (an empty patch: no source, so nothing is drawn until one is added).
//
// Each is a handful of cables (routes; anything not listed is off), mostly from the whole track's signals, not one a
// band (today's default chain has seven a band, 54 in all), and the few settings that make it what it is. The look
// (form, colours …) stays the track's own, as a preset leaves it.

import type { RouteValue } from './curves';
import { emptyExtra, type PatchExtra } from './patch';
import { defaultRoutes, routeKey } from './routes';
import type { VisualSettings } from './settings.svelte';

/** The settings a chain may set: how the frame behaves, the loop's and finish's levels, and whether the flame is drawn
 *  (`flameLayer`, its layer's amount: a5's, 0 from scratch); never the look. */
export type ChainSettings = Partial<
  Pick<
    VisualSettings,
    | 'flash'
    | 'speed'
    | 'smoothing'
    | 'fluid'
    | 'stir'
    | 'flow'
    | 'swirl'
    | 'waves'
    | 'trails'
    | 'tunnel'
    | 'dive'
    | 'effects'
    | 'facets'
    | 'fx1Amount'
    | 'fx2Amount'
    | 'fx3Amount'
    | 'fxLoopAmount'
  >
> & { flameLayer?: number };

export type Chain = { id: string; label: string; line: string; routes: Record<string, RouteValue>; extra: PatchExtra; settings?: ChainSettings };

/** None of the loop's and finish's own: what every chain starts from, so one chosen after another leaves none of it. */
const QUIET: ChainSettings = { flameLayer: 1, fluid: 0, tunnel: 0, dive: 0, effects: 0, facets: 0, waves: 0, fx1Amount: 0, fx2Amount: 0, fx3Amount: 0, fxLoopAmount: 0 };

export const CHAINS: readonly Chain[] = [
  {
    id: 'calm',
    label: 'Calm',
    line: 'Breathes with the loudness; its colours drift with the swells.',
    routes: {
      [routeKey('loudness', null, 'size')]: 0.6,
      [routeKey('loudness', null, 'saturation')]: 0.8,
      [routeKey('loudness', null, 'rotation')]: 0.15,
      [routeKey('swell', null, 'hue')]: 0.15,
    },
    extra: emptyExtra(),
    settings: { ...QUIET, flash: 0.3, speed: 0.6, smoothing: 0.6, trails: 1 },
  },
  {
    id: 'pulse',
    label: 'Pulse',
    line: 'The beat and the bass: size and light on every beat, the low end lighting its parts.',
    routes: {
      [routeKey('loudness', null, 'size')]: 0.7,
      [routeKey('loudness', null, 'saturation')]: 1,
      [routeKey('loudness', null, 'rotation')]: 0.3,
      [routeKey('beat', null, 'size')]: 0.25,
      [routeKey('beat', null, 'brightness')]: 0.4,
      [routeKey('downbeat', null, 'brightness')]: 0.3,
      [routeKey('hits', 0, 'lights')]: 1,
      [routeKey('hits', 1, 'lights')]: 0.8,
    },
    extra: emptyExtra(),
    settings: { ...QUIET, flash: 1, speed: 1 },
  },
  {
    id: 'flow',
    label: 'Flow',
    line: 'Smoke: the fluid stirred by the beats and the bass, whirled by the swells, little flashing.',
    routes: {
      [routeKey('beat', null, 'stir')]: 1,
      [routeKey('hits', 0, 'stir')]: 1,
      [routeKey('loudness', null, 'flow')]: 1,
      [routeKey('swell', null, 'swirl')]: 1,
      [routeKey('loudness', null, 'size')]: 0.6,
      [routeKey('loudness', null, 'rotation')]: 0.15,
    },
    extra: emptyExtra(),
    settings: { ...QUIET, fluid: 0.8, stir: 1, flow: 1, swirl: 1, flash: 0.3, speed: 0.8, trails: 1 },
  },
];

/** From scratch: nothing at all (the owner): no flame, no cables, no extras, the loop and finish off. */
export const SCRATCH: Chain = { id: 'scratch', label: 'Start from scratch', line: 'Nothing: add a source', routes: {}, extra: emptyExtra(), settings: { ...QUIET, flameLayer: 0 } };

export const chainOf = (id: string): Chain | undefined => (id === SCRATCH.id ? SCRATCH : CHAINS.find((c) => c.id === id));

/** A chain's routes as the settings keep them: every default route off (0) but those the chain sets. The settings keep
 *  their routes over the defaults (settings.svelte.ts `keptFrom`), so one left out would come back at the next load. */
export function chainRoutes(c: Chain, bands = 6): Record<string, RouteValue> {
  return { ...Object.fromEntries(Object.keys(defaultRoutes(bands)).map((k) => [k, 0])), ...c.routes };
}
