// The example racks (the owner, 2026-10-29: "maybe we just need a few more example snapshots for the different
// techniques and methods to get any sort of grid, kaleidoscope shape that we want"): the smallest racks that each show
// one technique, offered beside the snapshot library's presets. Each draws the sound (Waves, MilkDrop's own source)
// into the picture's loop (Feedback → … → Keep, the new picture over by the brighter), so what the technique does to
// the picture is what you see; each has a few macros on its face to play it. Data only, as devices.ts: the modules
// are the declared ones, the renderer flattens them.

import { MIX_MODES, type CableV2, type PatchV2 } from './ports';
import type { Module } from './patch';

/** An example: its id (`example:`…, never a snapshot's), its name and line in the presets, its rack. */
export type Example = { id: string; label: string; line: string; patch: PatchV2 };

const m = (id: string, type: string, knobs: Record<string, number> = {}): Module => ({ id, type, knobs });
const OVER = MIX_MODES.indexOf('over');
const LIGHTEN = MIX_MODES.indexOf('lighten');

type Macro = PatchV2['macros'][number];
type Rack = { modules: Module[]; cables: CableV2[]; macros: Macro[] };

function rack(id: string, label: string, line: string, build: (c: (from: string, fromPort: string, to: string, toPort: string, value?: number, channel?: number) => CableV2) => Rack): Example {
  let n = 0;
  const c = (from: string, fromPort: string, to: string, toPort: string, value = 1, channel?: number): CableV2 => ({
    id: `${id}-c${++n}`,
    from: { module: from, port: fromPort, ...(channel !== undefined ? { channel } : {}) },
    to: { module: to, port: toPort },
    value,
    depth: null,
  });
  const r = build(c);
  return { id: `example:${id}`, label, line, patch: { v: 2, modules: r.modules, cables: r.cables, macros: r.macros, pins: [] } };
}

const macro = (id: string, label: string, value: number, targets: Macro['targets']): Macro => ({ id, on: 'rack', label, value, targets });

/** Music, the Meter and the Screen, as every rack has them. */
const io = (): Module[] => [m('music', 'music'), m('meter', 'meter'), m('screen', 'screen')];

/** The picture's loop round `between` (Warps, Keeps, a Fold…): the new picture (on the Mix's first input) over the
 *  last one carried through them (its second), by the brighter; and the Mix to the Screen. */
function loop(c: Parameters<Parameters<typeof rack>[3]>[0], between: Module[]): { modules: Module[]; cables: CableV2[] } {
  const chain = ['feedback', ...between.map((x) => x.id)];
  return {
    modules: [m('mix', 'mix', { mode1: OVER, amount1: 1, mode2: LIGHTEN, amount2: 1 }), m('feedback', 'feedback'), ...between],
    cables: [
      ...chain.slice(1).map((id, i) => c(chain[i], 'picture', id, 'picture')),
      c(chain[chain.length - 1], 'picture', 'mix', 'in2'),
      c('mix', 'picture', 'feedback', 'picture'),
      c('mix', 'picture', 'screen', 'picture'),
    ],
  };
}

/** The sound drawn (Waves on Music's sound by its normal) and glowing, into the Mix's first input. */
function sound(c: Parameters<Parameters<typeof rack>[3]>[0], knobs: Record<string, number>): { modules: Module[]; cables: CableV2[] } {
  return {
    modules: [m('wave', 'waves', { width: 0.008, opacity: 0.9, amplitude: 0.35, saturation: 0.7, ...knobs }), m('glow', 'glow', { glow: 0.5 })],
    cables: [c('wave', 'picture', 'glow', 'picture'), c('glow', 'picture', 'mix', 'in1')],
  };
}

const keep = (amount = 0.9, trails = 0.75) => m('keep', 'keep', { amount, trails });

export const EXAMPLES: readonly Example[] = [
  rack('tunnel', 'Example · Tunnel', 'The edge zooms faster than the middle: the picture falls into a tunnel.', (c) => {
    const s = sound(c, { circle: 1, radius: 0.22 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.15, zoomEdge: 0.7 }), keep()]);
    return {
      modules: [...io(), ...s.modules, ...l.modules],
      cables: [...s.cables, ...l.cables],
      macros: [
        macro('depth', 'Depth', 0.7, [{ module: 'warp', knob: 'zoomEdge', from: 0, to: 1 }]),
        macro('speed', 'Speed', 0.3, [{ module: 'warp', knob: 'zoom', from: 0, to: 0.5 }]),
        macro('trail', 'Trail', 0.8, [{ module: 'keep', knob: 'trails', from: 0.3, to: 0.95 }]),
      ],
    };
  }),
  rack('whirlpool', 'Example · Whirlpool', 'The rim turns, the middle stays: the picture swirls into itself.', (c) => {
    const s = sound(c, { radius: 0.3 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.08, turnEdge: 0.6 }), keep()]);
    return {
      modules: [...io(), ...s.modules, ...l.modules],
      cables: [...s.cables, ...l.cables],
      macros: [
        macro('swirl', 'Swirl', 0.8, [{ module: 'warp', knob: 'turnEdge', from: -1, to: 1 }]),
        macro('pull', 'Pull', 0.2, [{ module: 'warp', knob: 'zoom', from: -0.2, to: 0.5 }]),
        macro('trail', 'Trail', 0.8, [{ module: 'keep', knob: 'trails', from: 0.3, to: 0.95 }]),
      ],
    };
  }),
  rack('petals', 'Example · Petals', 'Zooming in and out in petals round the middle, turning; their count glides.', (c) => {
    const s = sound(c, { circle: 1, radius: 0.18 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.05, petals: 0.5, petalCount: 5, petalTurn: 0.15 }), keep()]);
    return {
      modules: [...io(), ...s.modules, ...l.modules],
      cables: [...s.cables, ...l.cables],
      macros: [
        macro('count', 'Petals', 0.43, [{ module: 'warp', knob: 'petalCount', from: 2, to: 9 }]),
        macro('depth', 'Depth', 0.5, [{ module: 'warp', knob: 'petals', from: 0, to: 1 }]),
        macro('turn', 'Turn', 0.65, [{ module: 'warp', knob: 'petalTurn', from: -0.5, to: 0.5 }]),
      ],
    };
  }),
  rack('rings', 'Example · Bass rings', 'Each bass hit sends rings of zoom running out from the middle.', (c) => {
    const s = sound(c, { radius: 0.3 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.05, rings: 0.1, ringCount: 4, ringSpeed: 0.5 }), keep(0.9, 0.7)]);
    return {
      modules: [...io(), ...s.modules, ...l.modules, m('kick', 'gate', { above: 1, lift: 0.3, release: 0.3, amount: 1 })],
      // The bass's hits (Music's second band) through a beat gate at typical, onto the rings' depth.
      cables: [...s.cables, ...l.cables, c('music', 'hits', 'kick', 'in', 1, 1), c('kick', 'out', 'warp', 'rings', 0.6)],
      macros: [
        macro('count', 'Rings', 0.27, [{ module: 'warp', knob: 'ringCount', from: 1, to: 12 }]),
        macro('speed', 'Speed', 0.75, [{ module: 'warp', knob: 'ringSpeed', from: -1, to: 1 }]),
        macro('trail', 'Trail', 0.7, [{ module: 'keep', knob: 'trails', from: 0.3, to: 0.95 }]),
      ],
    };
  }),
  rack('wandering', 'Example · Wandering heart', 'The tunnel’s heart wanders off the middle, moved by time, not the music.', (c) => {
    const s = sound(c, { circle: 1, radius: 0.2 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.2, zoomEdge: 0.4, turn: 0.05 }), keep()]);
    return {
      modules: [...io(), ...s.modules, ...l.modules, m('across', 'random', { rate: 0.08, amount: 1 }), m('up', 'random', { rate: 0.06, amount: 1 })],
      cables: [...s.cables, ...l.cables, c('across', 'out', 'warp', 'centreX', 0.5), c('up', 'out', 'warp', 'centreY', 0.5)],
      macros: [
        macro('depth', 'Depth', 0.4, [{ module: 'warp', knob: 'zoomEdge', from: 0, to: 1 }]),
        macro('wander', 'Wander', 0.3, [
          { module: 'across', knob: 'rate', from: 0.02, to: 0.3 },
          { module: 'up', knob: 'rate', from: 0.015, to: 0.25 },
        ]),
      ],
    };
  }),
  rack('kaleido-feedback', 'Example · Kaleidoscope in the loop', 'Each fold fed back and folded again, the petals turning it.', (c) => {
    const s = sound(c, { radius: 0.25 });
    const l = loop(c, [m('fold', 'fold', { mode: 1, count: 6, amount: 1 }), m('warp', 'warp', { amount: 1, zoom: 0.1, turn: 0.05, petals: 0.2, petalCount: 6 }), keep(0.8, 0.6)]);
    return {
      modules: [...io(), ...s.modules, ...l.modules],
      cables: [...s.cables, ...l.cables],
      macros: [
        macro('folds', 'Folds', 0.33, [{ module: 'fold', knob: 'count', from: 3, to: 12 }]),
        macro('petals', 'Petals', 0.2, [{ module: 'warp', knob: 'petals', from: 0, to: 1 }]),
        macro('depth', 'Depth', 0.6, [{ module: 'keep', knob: 'amount', from: 0.4, to: 0.95 }]),
      ],
    };
  }),
  rack('grid-cells', 'Example · Kaleidoscope grid', 'A hexagonal grid, each cell its own kaleidoscope of the sound.', (c) => {
    const s = sound(c, { radius: 0.3, amplitude: 0.5 });
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.06, turnEdge: 0.15 }), keep(0.85, 0.6)]);
    return {
      modules: [...io(), ...s.modules, ...l.modules, m('grid', 'grid', { lattice: 1, count: 19, spacing: 0.3 }), m('fold', 'fold', { mode: 1, count: 6, amount: 1 })],
      // The sound folded in the grid's cells before it comes into the loop.
      cables: [
        ...s.cables.filter((x) => x.to.module !== 'mix'),
        ...l.cables,
        c('glow', 'picture', 'fold', 'picture'),
        c('grid', 'points', 'fold', 'grid'),
        c('fold', 'picture', 'mix', 'in1'),
      ],
      macros: [
        macro('cells', 'Cells', 0.25, [{ module: 'grid', knob: 'count', from: 4, to: 64 }]),
        macro('spacing', 'Spacing', 0.3, [{ module: 'grid', knob: 'spacing', from: 0.1, to: 0.8 }]),
        macro('folds', 'Folds', 0.33, [{ module: 'fold', knob: 'count', from: 3, to: 12 }]),
      ],
    };
  }),
  rack('shapes-grid', 'Example · Shapes on a grid', 'A shape at every point of a spiral grid, swirled by the loop.', (c) => {
    const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.04, turnEdge: 0.3 }), keep(0.85, 0.65)]);
    return {
      modules: [
        ...io(),
        ...l.modules,
        m('grid', 'grid', { lattice: 4, count: 48, spacing: 0.12 }),
        m('dot', 'shape', { kind: 1, sides: 6, size: 0.04, hue: 0.55, saturation: 0.6, brightness: 1.2 }),
        m('each', 'instance', { amount: 1 }),
        m('pulse', 'envelope', { attack: 0.05, release: 0.4 }),
      ],
      cables: [
        ...l.cables,
        c('grid', 'points', 'each', 'points'),
        c('dot', 'picture', 'each', 'picture'),
        c('each', 'picture', 'mix', 'in1'),
        // The loudness breathes the shapes' size.
        c('music', 'loudness', 'pulse', 'in'),
        c('pulse', 'level', 'dot', 'size', 0.5),
      ],
      macros: [
        macro('count', 'Count', 0.7, [{ module: 'grid', knob: 'count', from: 8, to: 64 }]),
        macro('spacing', 'Spacing', 0.2, [{ module: 'grid', knob: 'spacing', from: 0.05, to: 0.4 }]),
        macro('swirl', 'Swirl', 0.65, [{ module: 'warp', knob: 'turnEdge', from: -1, to: 1 }]),
      ],
    };
  }),
];

export const exampleOf = (id: string): Example | undefined => EXAMPLES.find((x) => x.id === id);
