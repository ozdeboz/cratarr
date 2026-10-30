// What each of the light's stages draws with this frame (the points, the light's loop, Show, the finish): the stages'
// own modules' values (the engine's `params`, each module's node) where the patch has them, and what the Cratarr
// device gives where it has none. Which module of a type a stage reads: the light loop's and the Flame's, the first of
// each in the patch; Show's, the compiled graph's parts on the light. The renderer's (render.ts), and the golden
// frames' (tests/harness/golden.ts), so what is proved is what is drawn. Pure.
import type { ChainNow, ShowParams } from './chain-params';
import type { NodeParams } from './engine/graph';
import { compile, stageOf, stagesOf, type TypeOf } from './picture-graph';
import type { PatchV2 } from './ports';
import { flatten } from './v2';
import { DIVE_MOST, TUNNEL_MOST } from './zoom';

/** The light loop's modules and the Flame. */
const LOOP_TYPES = new Set(['trails', 'fluid', 'tunnel', 'dive', 'flame']);

/** Which module each stage reads, by type: Show's parts on the light (as compiled), the light loop's and the Flame. */
export type StageIds = { show: Map<string, string>; loop: Map<string, string> };
export function stageIdsOf(p: PatchV2, typeOf: TypeOf): StageIds {
  const c = compile(p, typeOf);
  const stages = stagesOf(c);
  const loop = new Map<string, string>();
  for (const m of flatten(p).modules) if (LOOP_TYPES.has(m.type) && !loop.has(m.type)) loop.set(m.type, m.id);
  return { show: new Map(stages ? c.order.filter((n) => stageOf(n) === 'show').map((n) => [n.type, n.id]) : []), loop };
}

/** Show's values with its parts' own (Tone map's flash and exposure, Saturate's, Curve's gamma and floor, Blur's). */
export function ownShow(look: ShowParams, of: (type: string) => NodeParams | undefined): ShowParams {
  const [tone, sat, curve, blur] = [of('tonemap'), of('saturate'), of('curve'), of('blur')];
  return {
    ...look,
    ...(tone?.flash !== undefined ? { flash: tone.flash } : {}),
    ...(tone?.exposure !== undefined ? { exposure: tone.exposure } : {}),
    ...(sat?.saturation !== undefined ? { saturation: sat.saturation } : {}),
    ...(curve?.gamma !== undefined ? { gamma: curve.gamma, floor: curve.floor ?? 0 } : {}),
    ...(blur?.smoothing !== undefined ? { smoothing: blur.smoothing } : {}),
  };
}

/** The light loop's and the camera's values with each module's own where it has them: Trails' keep, the Fluid's,
 *  Tunnel's, Dive's, the Flame's size. */
export function ownLoop(now: ChainNow, of: (type: string) => NodeParams | undefined): ChainNow {
  const [trails, fluid, tunnel, dive, flame] = [of('trails'), of('fluid'), of('tunnel'), of('dive'), of('flame')];
  const keep = trails?.keep ?? now.carry.keep;
  return {
    ...now,
    points: {
      ...now.points,
      ...(flame?.scale !== undefined ? { scale: flame.scale } : {}),
      ...(flame?.flame ? { turn: flame.flame.turn } : {}),
      ...(dive?.dive !== undefined ? { dive: DIVE_MOST * dive.dive } : {}),
    },
    carry: {
      ...now.carry,
      keep,
      ...(fluid?.fluid !== undefined ? { fluid: fluid.fluid, warp: fluid.warp ?? 0, facets: { hex: now.carry.facets?.hex ?? false, amount: fluid.facets ?? 0 } } : {}),
      ...(tunnel?.tunnel !== undefined ? { tunnel: TUNNEL_MOST * tunnel.tunnel } : {}),
    },
    show: { ...now.show, keep, ...(flame?.flame ? { hits: flame.flame.hits } : {}) },
  };
}

/** The stages' values as drawn: the device's, each stage's own modules' over them. */
export function stagesNow(now: ChainNow, ids: StageIds, params: (id: string) => NodeParams | undefined): ChainNow {
  const loop = ownLoop(now, (type) => (ids.loop.has(type) ? params(ids.loop.get(type)!) : undefined));
  return { ...loop, show: ownShow(loop.show, (type) => (ids.show.has(type) ? params(ids.show.get(type)!) : undefined)) };
}
