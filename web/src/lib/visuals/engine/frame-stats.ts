// A frame's numbers for the meters (meters.ts FrameStats, but its tiles, which the GPU reads): how the parts turn and
// change, the picture's size and colourfulness, what the effects, the waves and the zooms move. The frame loop's
// (visuals-loop.ts), and the golden frames' closed loop (tests/harness/golden.ts), from the engine's frame alike.
import { wavesMotion, zoomsMotion } from '../aspects';
import { settingsReport, type EffectSettings } from '../effects';
import type { FrameStats } from '../meters';
import type { PatchV2 } from '../ports';
import { A } from '../routes';
import type { EngineFrame } from './graph';
import type { Levers } from './sidechain';

export type StatsContext = {
  /** The patch the engine ran (for its Bend's waves), the effects' slots, the levers, the time (s). */
  patch: PatchV2;
  how: EffectSettings & { lattice: number };
  levers: Levers;
  t: number;
  /** A new look this frame, and why. */
  look: FrameStats['look'];
  /** What the dive turns each part by besides (the renderer's), radians a second. */
  diveTurn?: number;
};

/** The frame's stats; null without the Cratarr device. */
export function frameStats(f: EngineFrame, c: StatsContext): Omit<FrameStats, 'tiles'> | null {
  const d = f.aspects;
  if (!d) return null;
  const flameId = c.patch.modules.find((x) => x.type === 'flame')?.id;
  const fl = (flameId ? f.params[flameId]?.flame : undefined) ?? d;
  const bendId = c.patch.modules.find((x) => x.type === 'bend')?.id;
  const waves = (bendId ? (f.params[bendId]?.waves ?? d.given[A.waves]) : d.given[A.waves]) * c.levers.pace;
  const fx = settingsReport(c.how, d.effects, c.levers, d.routed);
  const zooms = zoomsMotion(d.tunnel * c.levers.pace, d.dive * c.levers.pace);
  return {
    t: c.t,
    extra: { ...fx, motion: (fx.motion ?? 0) + wavesMotion(waves, d.fluid) + zooms },
    turnRates: c.diveTurn ? fl.rates.map((r) => r + c.diveTurn!) : fl.rates,
    shape: fl.shape,
    weights: fl.parts.map((p) => p.weight ?? 1),
    hue: fl.parts.map((p) => p.hue),
    spread: fl.parts.map((p) => p.spread ?? 1),
    scale: d.scale,
    saturation: d.saturation,
    look: c.look,
    symmetry: d.nexts.symmetry,
  };
}
