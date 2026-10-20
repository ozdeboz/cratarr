// The chain's values for the renderer's nodes (decision 0057, step 3): what the mapping gives this frame (aspects.ts's
// AspectsNow, as claude-5f's engine will) and the frame's own (ChainContext) grouped into what each node reads: the
// points, the flame's own loop on the light, the show, the effects. Pure. The shaping of the swings into sizes and
// gains stays where the course filter runs (aspects.ts) until the engine gives knobs in knob units.
import type { AspectsNow } from './aspects';
import { effectsNow, type Beat, type EffectSettings } from './effects';
import type { Flow, Force } from './fluid';
import type { Levers } from './engine/sidechain';
import type { Shown } from './render';
import { DIVE_MOST, TUNNEL_MOST } from './zoom';

/** The points' camera: their size and turn, and the dive's rate. */
export type PointsParams = Pick<Shown, 'scale' | 'turn' | 'dive'>;
/** The flame's own loop on the light: the share kept, the fluid (its amount, read-back, pushes, flow, step), the
 *  tunnel's rate, the facets. */
export type CarryParams = Pick<Shown, 'keep' | 'fluid' | 'warp' | 'tunnel' | 'facets' | 'forces' | 'flow' | 'dt'>;
/** The show: the display's curve and gains, the bands' lights, the fire, the ceiling; the share kept (the points
 *  held at a time) and the waves' bend. */
export type ShowParams = Pick<Shown, 'brightness' | 'keep' | 'gamma' | 'vibrancy' | 'zoom' | 'flash' | 'hits' | 'ceiling' | 'exposure' | 'saturation' | 'smoothing' | 'floor' | 'fire' | 'flow'>;
/** The effects: the chain's and the loop's. */
export type EffectsParams = Shown['effects'];

export type ChainNow = { points: PointsParams; carry: CarryParams; show: ShowParams; effects: EffectsParams };

/** The frame's own, beside the mapping: the effects' and facets' settings, the sidechaining's levers, reduced motion, the
 *  most brightness, how much of its frame the flame covers (null till measured), the fire's share, the track's time
 *  and beat, the fluid's pushes and flow, the frame's length and the Speed setting as moved. */
export type ChainContext = {
  how: EffectSettings & { lattice: number };
  levers: Levers;
  ceiling: number;
  coverage: number | null;
  fire: number;
  position: number;
  beat: Beat | null;
  forces: Force[];
  flow: Flow;
  dt: number;
  speed: number;
};

/** This frame's values for the nodes. */
export function chainNow(d: AspectsNow, c: ChainContext): ChainNow {
  const pace = c.levers.pace;
  return {
    points: {
      turn: d.turn,
      scale: d.scale,
      // The dive (zoom.ts): as fast as its routes let its slider, held back with the pace; none with reduced motion.
      dive: DIVE_MOST * d.dive * pace,
    },
    carry: {
      keep: d.keep,
      fluid: d.fluid,
      // The tunnel likewise.
      tunnel: TUNNEL_MOST * d.tunnel * pace,
      facets: { amount: d.facets, hex: Math.round(c.how.lattice) === 1 },
      forces: c.forces,
      flow: c.flow,
      // At the Speed setting's pace, as the turning (a quarter with reduced motion; none at 0). The fluid takes at most
      // 0.05 s a step: at 30 frames a second, Speed past 150 % flows no faster.
      dt: c.dt * c.speed * pace,
      warp: d.warp,
    },
    show: {
      brightness: 1,
      keep: d.keep,
      gamma: d.gamma,
      vibrancy: 1,
      zoom: 1,
      flash: d.flash,
      hits: d.hits,
      ceiling: c.ceiling,
      saturation: d.saturation,
      // The whole picture as bright however much of its frame the flame covers: a flame spread over most of it is
      // shown dimmer, one gathered into a few cells brighter (against one covering a third of its frame), so its
      // shape's changes are not flares.
      exposure: c.coverage !== null ? Math.min(1.5, Math.max(0.4, 0.33 / Math.max(0.05, c.coverage))) : 1,
      smoothing: d.smoothing,
      floor: d.floor,
      fire: c.fire,
      flow: c.flow,
    },
    // The look's effects, as far as the Effects setting and its routes let them, moving with the track's time.
    effects: effectsNow(c.how, d.effects, c.position, c.beat, c.levers, d.routed),
  };
}

/** The nodes' values as one look (the frozen renderer's, the lab's), and back. */
export const shownOf = (n: ChainNow): Shown => ({ ...n.show, ...n.carry, ...n.points, effects: n.effects });
export const chainOfShown = (l: Shown): ChainNow => ({
  points: { scale: l.scale, turn: l.turn, dive: l.dive },
  carry: { keep: l.keep, fluid: l.fluid, warp: l.warp, tunnel: l.tunnel, facets: l.facets, forces: l.forces, flow: l.flow, dt: l.dt },
  show: {
    brightness: l.brightness,
    keep: l.keep,
    gamma: l.gamma,
    vibrancy: l.vibrancy,
    zoom: l.zoom,
    flash: l.flash,
    hits: l.hits,
    ceiling: l.ceiling,
    exposure: l.exposure,
    saturation: l.saturation,
    smoothing: l.smoothing,
    floor: l.floor,
    fire: l.fire,
    flow: l.flow,
  },
  effects: l.effects,
});
