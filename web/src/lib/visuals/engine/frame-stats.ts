// A frame's numbers for the meters (meters.ts FrameStats, but its tiles, which the GPU reads): how the parts turn and
// change, the picture's size and colourfulness, what the effects, the waves and the zooms move. From the engine's
// frame (its Flame's node, its device's stages and meter: 0.10, no summary between), for the frame loop
// (visuals-loop.ts) and the golden frames' closed loop (tests/harness/golden.ts) alike.
import { WAVE_BEND } from '../fluid';
import type { FrameStats } from '../meters';
import type { PatchV2 } from '../ports';
import { DIVE_MOST, TUNNEL_MOST } from '../zoom';
import type { EngineFrame } from './graph';

/** How fast the slow giant waves move the picture where nothing flows (the show pass bends it by `WAVE_BEND` of the
 *  screen's height, once round their cycle in eight seconds: stirring.ts), as a turn a second of the flame's parts
 *  (whose points lie about 0.4 of the screen's height out): what the motion meter counts for them. Where the fluid
 *  flows they are part of its flow, which the meters do not see (the levers' pace slows it all the same). */
export const wavesMotion = (amount: number, fluid: number) => (fluid > 0 ? 0 : (WAVE_BEND * Math.abs(amount) * ((2 * Math.PI) / 8)) / 0.4);

/** How fast the zooms stream the picture out (zoom.ts: each speed its log a second): a point at radius r moves at that
 *  many e-folds a second times r, as a turn of as many radians a second moves it, so the motion meter counts the Tunnel's
 *  and the Dive's zoom (the Dive's turn beside it) as that much motion. `tunnel`, `dive`: their levels now (0..1). */
export const zoomsMotion = (tunnel: number, dive: number) => TUNNEL_MOST * Math.max(0, tunnel) + DIVE_MOST * Math.max(0, dive);

export type StatsContext = {
  /** The patch the engine ran (where its Flame is), the time (s). */
  patch: PatchV2;
  t: number;
  /** A new look this frame, and why. */
  look: FrameStats['look'];
  /** What the dive turns each part by besides (the renderer's), radians a second. */
  diveTurn?: number;
};

/** The frame's stats; null without the Cratarr device. */
export function frameStats(f: EngineFrame, c: StatsContext): Omit<FrameStats, 'tiles'> | null {
  const d = f.device;
  const flameId = c.patch.modules.find((x) => x.type === 'flame')?.id;
  const fl = flameId ? f.params[flameId]?.flame : undefined;
  if (!d || !fl) return null;
  const fx = d.meter.effects;
  return {
    t: c.t,
    extra: { ...fx, motion: (fx.motion ?? 0) + wavesMotion(d.meter.waves, d.meter.fluid) + zoomsMotion(d.meter.tunnel, d.meter.dive) },
    turnRates: c.diveTurn ? fl.rates.map((r) => r + c.diveTurn!) : fl.rates,
    shape: fl.shape,
    weights: fl.parts.map((p) => p.weight ?? 1),
    hue: fl.parts.map((p) => p.hue),
    spread: fl.parts.map((p) => p.spread ?? 1),
    scale: d.stages.points.scale,
    saturation: d.stages.show.saturation,
    look: c.look,
    symmetry: d.meter.symmetry,
  };
}
