// The visuals' preset chains (decision 0056: "chains are presets"). The owner, after 70: "we should also have some
// sensible presets, but nothing extreme the way we now have. Currently we have way too many lines. So we need a few
// sensible chains as presets, the ability to start from scratch"; asked which: Calm, Pulse and Flow; and from scratch,
// nothing at all (an empty patch: no source, so nothing is drawn until one is added).
//
// Each is a handful of cables (routes; anything not listed is off), mostly from the whole track's signals, not one a
// band (today's default chain has seven a band, 54 in all), and the few settings that make it what it is. The look
// (form, colours …) stays the track's own, as a preset leaves it.

import type { RouteValue } from './curves';
import { emptyExtra, type Cable, type Module, type PatchExtra } from './patch';
import { extraType } from './registry';
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

// What the preset study taught (9,109 of Cream of the Crop's presets, read for how they are made, none of them copied;
// visuals-milkdrop-direction): a preset makes its own modulators first and drives through them, not the raw signals;
// its beat test sits at typical (our gate's Above 1); the music drives the drawing and the zoom, while turning and
// drifting go with time; decay and finish stay put; the look follows the slow inputs, only the hits the fast ones;
// most run the picture's own loop (warp and keep). Each chain below holds to that, in at most eight cables.

/** One of the extras' modules at its type's defaults, some set (`over`). */
function mod(type: string, id: string, over: Record<string, number> = {}): Module {
  const t = extraType(type);
  return { id, type, knobs: { ...Object.fromEntries((t?.knobs ?? []).map((k) => [k.id, k.default])), ...over } };
}
const cable = (id: string, from: Cable['from'], module: string, knob: string, value: number, mode: Cable['mode'] = 'swing'): Cable => ({ id, from, to: { module, knob }, value, mode });

export const CHAINS: readonly Chain[] = [
  {
    // A slow follower of the loudness breathes the size and the colour's strength; the turning wanders with time; the
    // hue moves with the swells, slowly.
    id: 'calm',
    label: 'Calm',
    line: 'Breathes with the loudness; its colours drift with the swells.',
    routes: {},
    extra: {
      ...emptyExtra(),
      modulators: [mod('slow', 'breath', { attack: 0.8, release: 2.5 }), mod('drift', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('calm-in', { signal: 'loudness' }, 'breath', 'in', 1),
        cable('calm-size', { modulator: 'breath' }, 'show', 'size', 0.5),
        cable('calm-saturation', { modulator: 'breath' }, 'show', 'saturation', 0.6),
        cable('calm-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.3),
        cable('calm-hue', { signal: 'swell', speed: 'slow' }, 'flame', 'hue', 0.15),
      ],
    },
    settings: { ...QUIET, flash: 0.3, speed: 0.6, smoothing: 0.6, trails: 1 },
  },
  {
    // The bass's hits through a beat gate (at typical, lifting after each) push the size and the glow; a fast follower
    // of the loudness the colour; the downbeats glow; the sub's hits light the flame's parts; the turning wanders.
    id: 'pulse',
    label: 'Pulse',
    line: 'The beat and the bass: size and light on every hit, the low end lighting its parts.',
    routes: {
      [routeKey('downbeat', null, 'brightness')]: 0.3,
      [routeKey('hits', 0, 'lights')]: 1,
    },
    extra: {
      ...emptyExtra(),
      modulators: [mod('gate', 'kick', { above: 1, lift: 0.3, release: 0.15, amount: 1 }), mod('fast', 'body'), mod('drift', 'wander', { rate: 0.1, amount: 1 })],
      cables: [
        cable('pulse-kick-in', { signal: 'hits', band: 1 }, 'kick', 'in', 1),
        cable('pulse-body-in', { signal: 'loudness' }, 'body', 'in', 1),
        cable('pulse-size', { modulator: 'kick' }, 'show', 'size', 0.3),
        cable('pulse-glow', { modulator: 'kick' }, 'show', 'glow', 0.4),
        cable('pulse-saturation', { modulator: 'body' }, 'show', 'saturation', 0.8),
        cable('pulse-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.3),
      ],
    },
    settings: { ...QUIET, flash: 1, speed: 1 },
  },
  {
    // Smoke: the fluid stirred by the beats and the sub, flowing with the loudness (slowly), whirled by the swells; the
    // picture's own loop keeps a trail of it, the bass's hits zooming it in, its turn wandering.
    id: 'flow',
    label: 'Flow',
    line: 'Smoke: the fluid stirred by the beats and the bass, whirled by the swells, little flashing.',
    routes: {
      [routeKey('beat', null, 'stir')]: 1,
      [routeKey('hits', 0, 'stir')]: 1,
      [routeKey('swell', null, 'swirl')]: 1,
    },
    extra: {
      ...emptyExtra(),
      loop: [mod('warp', 'flow-warp', { zoom: 0.05 }), mod('keep', 'flow-keep', { amount: 0.6, trails: 0.5 })],
      modulators: [mod('drift', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('flow-flow', { signal: 'loudness', speed: 'slow' }, 'fluid', 'flow', 1),
        cable('flow-zoom', { signal: 'hits', band: 1 }, 'flow-warp', 'zoom', 0.2),
        cable('flow-turn', { modulator: 'wander' }, 'flow-warp', 'turn', 0.3),
      ],
    },
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
