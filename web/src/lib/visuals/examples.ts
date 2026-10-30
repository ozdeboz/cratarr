// The example racks (the owner, 2026-10-29: "maybe we just need a few more example snapshots for the different
// techniques and methods to get any sort of grid, kaleidoscope shape that we want"): the smallest racks that each show
// one technique, in the snapshot library (library.ts) under Examples. Each draws the sound (Waves, MilkDrop's own source)
// into the picture's loop (Feedback → … → Keep, the new picture over by the brighter), so what the technique does to
// the picture is what you see; each has a few macros on its face to play it. Data only, as devices.ts: the modules
// are the declared ones, the renderer flattens them.

import { MIX_MODES, RANDOMIZE_SCOPES, type CableV2, type PatchV2 } from './ports';
import { DEFAULTS, type VisualSettings } from './settings.svelte';
import { toV2 } from './v2';
import { FLAME_BASE } from './examples-flame';
import type { Module } from './patch';

/** An example: its id (`example:`…, never a snapshot's), its name and line in the library, its rack. */
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


/** The test rack (2026-10-30): "Wandering kaleidoscope" with every knob the Everything dice reaches at the
 *  middle of its range and every macro at half (the average a held-down Everything hovers around), standing still: the
 *  Wander dice unplugged and the drift off. Does the middle look as good as the pressing, or does the pressing need the
 *  motion? */
const MIDDLE: Record<string, Record<string, number>> = {
  "wave": {"width": 0.025, "opacity": 0.5, "amplitude": 0.5, "circle": 0.5, "radial": 0.5, "radius": 0.5, "length": 0.5, "x": 0, "y": 0, "rotation": 0.5, "smoothing": 0.5, "hue": 0.5, "hueSpread": 0.5, "saturation": 0.5},
  "glow": {"glow": 0.5},
  "mix": {"amount1": 0.5, "amount2": 0.5, "amount3": 0.5, "amount4": 0.5},
  "warp": {"amount": 0.5, "zoom": 0, "turn": 0, "driftX": 0, "driftY": 0, "stretch": 0, "wobble": 0.5, "wobbleSize": 0.5, "centreX": 0, "centreY": 0, "zoomEdge": 0, "turnEdge": 0, "petals": 0, "petalCount": 6, "petalTurn": 0, "rings": 0, "ringCount": 6.25, "ringSpeed": 0, "turbulence": 0.5, "turbulenceSpeed": 1, "turbulenceSize": 2.125},
  "keep": {"amount": 0.5, "trails": 0.5},
  "grid": {"spacing": 0.51, "rotation": 0.5, "jitter": 0.5, "drift": 1, "return": 1.5, "cover": 1.5},
  "fold": {"turn": 0, "amount": 0.5, "zoom": 2.125, "edge": 0.025, "centre": 0.5, "reach": 2.125, "lift": 0.5, "vary": 0.5, "gridSpin": 0, "breathe": 0.5},
  "clock": {"speed": 1, "beats": 66},
  "form-drift": {"rate": 1, "shape": 0.5},
  "size-drift": {"rate": 1, "shape": 0.5},
  "angle-drift": {"rate": 1, "shape": 0.5},
};
const middled = (e: Example): Example => ({
  id: 'example:wandering-kaleido-middle',
  label: 'Wandering kaleidoscope · the middle, still',
  line: 'Every knob at the middle of its range, nothing moving but the music: the average a held-down dice shows.',
  patch: {
    ...e.patch,
    modules: e.patch.modules.map((x) => ({ ...x, knobs: { ...x.knobs, ...(MIDDLE[x.id] ?? {}) } })),
    cables: e.patch.cables.filter((x) => x.to.module !== 'wander'),
    macros: e.patch.macros.map((x) => ({ ...x, value: x.id === 'drift' ? 0 : 0.5 })),
  },
});

const BUILT: Example[] = [
  rack('tunnel', 'Tunnel', 'The edge zooms faster than the middle: the picture falls into a tunnel.', (c) => {
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
  rack('whirlpool', 'Whirlpool', 'The rim turns, the middle stays: the picture swirls into itself.', (c) => {
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
  rack('petals', 'Petals', 'Zooming in and out in petals round the middle, turning; their count glides.', (c) => {
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
  rack('rings', 'Bass rings', 'Each bass hit sends rings of zoom running out from the middle.', (c) => {
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
  rack('wandering', 'Wandering heart', 'The tunnel’s heart wanders off the middle, moved by time, not the music.', (c) => {
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
  rack('kaleido-feedback', 'Kaleidoscope in the loop', 'Each fold fed back and folded again, the petals turning it.', (c) => {
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
  rack('gradient-field', 'A field from a gradient', 'A gradient’s light pushes the loop: out from the middle, round in petals.', (c) => {
    const s = sound(c, { radius: 0.25 });
    const l = loop(c, [m('push', 'displace', { amount: 0.4, scale: 0.6, zoom: 0.3, turn: 0.2 }), keep(0.88, 0.7)]);
    return {
      modules: [...io(), ...s.modules, ...l.modules, m('field', 'gradient', { distance: 0.6, round: 0.5, roundCount: 3, repeat: 1.5, speed: 0.1 })],
      // The gradient pushes what the loop carries: along its light's rise, and zoomed and turned by it.
      cables: [...s.cables, ...l.cables, c('field', 'picture', 'push', 'by')],
      macros: [
        macro('petals', 'Petals', 0.25, [{ module: 'field', knob: 'roundCount', from: 0, to: 12 }]),
        macro('push', 'Push', 0.4, [{ module: 'push', knob: 'amount', from: 0, to: 1 }]),
        macro('swirl', 'Swirl', 0.6, [{ module: 'push', knob: 'turn', from: -1, to: 1 }]),
      ],
    };
  }),
  rack('grid-cells', 'Kaleidoscope grid', 'A hexagonal grid, each cell its own kaleidoscope of the sound.', (c) => {
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
      // The Grid is laid out from its own knobs (graph.ts reads neither cables nor macros there yet): its cells set by hand.
      macros: [
        macro('folds', 'Folds', 0.33, [{ module: 'fold', knob: 'count', from: 3, to: 12 }]),
        macro('swirl', 'Swirl', 0.57, [{ module: 'warp', knob: 'turnEdge', from: -1, to: 1 }]),
        macro('trail', 'Trail', 0.5, [{ module: 'keep', knob: 'trails', from: 0.3, to: 0.95 }]),
      ],
    };
  }),
  // 2026-10-30 (the owner's snapshot "4!!" taken further): a honeycomb of kaleidoscopes filling the screen,
  // every cell on the light with its own variation, the grid slowly turning, breathing and melting, and a Dice on Spatial
  // fired each phrase whose rolls glide in over 15 s, so the space wanders and never stands still. The knobs that step
  // (lattice, fit, fold mode and count) are pinned, so no morph jumps the structure.
  ((e: Example): Example => ({ ...e, patch: { ...e.patch, pins: ['grid.lattice', 'grid.fit', 'grid.count', 'fold.mode', 'fold.count'] } }))(
    rack('wandering-kaleido', 'Wandering kaleidoscope', 'A honeycomb of kaleidoscopes, the space gliding to somewhere new each phrase.', (c) => {
      const s = sound(c, { x: 0, y: 0, circle: 0.6, radius: 0.3, amplitude: 0.4, width: 0.02, spin: 0.03 });
      const l = loop(c, [m('warp', 'warp', { amount: 1, zoom: 0.04, turnEdge: 0.1 }), keep(0.85, 0.6)]);
      return {
        modules: [
          ...io(),
          ...s.modules,
          ...l.modules,
          m('grid', 'grid', { lattice: 1, count: 36, fit: 1, cover: 1.3, jitter: 0.05, rotation: 0.05, return: 3 }),
          m('fold', 'fold', { mode: 1, count: 11, amount: 1, centre: 1, vary: 0.5, reach: 2.1, lift: 0.4, edge: 0.03, gridSpin: 0.15, breathe: 0.4, melt: 0.25 }),
          m('clock', 'clock'),
          m('wander', 'dice', { scope: RANDOMIZE_SCOPES.indexOf('spatial'), reach: 0.5, glide: 15 }),
          // The source's own shape drifting all the time, slowly and smoothly (Random at Shape 0): with Centre at 1 every
          // cell follows the light, so where the source is never shows; its form does: round to line, size, swing,
          // length and angle.
          m('form-drift', 'random', { rate: 0.07, shape: 0, amount: 1 }),
          m('size-drift', 'random', { rate: 0.05, shape: 0, amount: 1 }),
          m('angle-drift', 'random', { rate: 0.04, shape: 0, amount: 1 }),
        ],
        cables: [
          ...s.cables.filter((x) => x.to.module !== 'mix'),
          ...l.cables,
          c('glow', 'picture', 'fold', 'picture'),
          c('grid', 'points', 'fold', 'grid'),
          c('fold', 'picture', 'mix', 'in1'),
          c('clock', 'phrase', 'wander', 'trigger'),
          c('form-drift', 'out', 'wave', 'circle', 0.5),
          c('form-drift', 'out', 'wave', 'radial', 0.3),
          c('size-drift', 'out', 'wave', 'radius', 0.4),
          c('size-drift', 'out', 'wave', 'amplitude', 0.3),
          c('angle-drift', 'out', 'wave', 'rotation', 0.5),
          c('angle-drift', 'out', 'wave', 'length', 0.4),
        ],
        macros: [
          macro('wander', 'Wander', 0.6, [{ module: 'wander', knob: 'reach', from: 0, to: 0.8 }]),
          macro('drift', 'Drift', 0.7, [
            { module: 'form-drift', knob: 'amount', from: 0, to: 1 },
            { module: 'size-drift', knob: 'amount', from: 0, to: 1 },
            { module: 'angle-drift', knob: 'amount', from: 0, to: 1 },
          ]),
          macro('calm', 'Calm', 0.8, [{ module: 'wave', knob: 'spin', from: 0.3, to: 0 }]),
          macro('melt', 'Melt', 0.25, [{ module: 'fold', knob: 'melt', from: 0, to: 1 }]),
        ],
      };
    }),
  ),
  // The recipe from the owner's "Damn!!" (2026-10-30): a fresh picture every frame (the loop barely keeps
  // anything), a bright compact source whose form keeps changing quickly, every cell copying only its light (Amount 1,
  // Centre near 1, a wide Reach), and the whole structure moving slowly (the grid turning, breathing, melting).
  rack('sunbursts', 'Sunbursts', 'Crisp sunbursts in every cell: the sound a spoked circle that never keeps its shape.', (c) => {
    const s = sound(c, { x: 0, y: 0, circle: 0.7, radial: 0.7, radius: 0.9, amplitude: 0.35, width: 0.02, opacity: 0.85, hueSpread: 0.75, spin: 0.03 });
    const l = loop(c, [m('warp', 'warp', { amount: 0.5, zoom: 0.05 }), keep(0.12, 0.13)]);
    return {
      modules: [
        ...io(),
        ...s.modules,
        ...l.modules,
        m('grid', 'grid', { lattice: 1, count: 36, fit: 1, cover: 1.15, jitter: 0.2, return: 3 }),
        m('fold', 'fold', { mode: 1, count: 12, amount: 1, centre: 0.85, vary: 0.2, reach: 3, lift: 0.4, edge: 0.02, gridSpin: -0.5, breathe: 0.8, melt: 0.5 }),
        m('form-drift', 'random', { rate: 1.6, shape: 0.2, amount: 0.8 }),
        m('size-drift', 'random', { rate: 0.5, shape: 0.1, amount: 0.8 }),
        m('angle-drift', 'random', { rate: 0.25, shape: 0, amount: 0.8 }),
        // The source travelling round the screen: a new place every 20 to 30 s, across and up at their own pace.
        m('path-x', 'random', { rate: 0.05, shape: 0, amount: 0.8 }),
        m('path-y', 'random', { rate: 0.035, shape: 0, amount: 0.8 }),
      ],
      cables: [
        ...s.cables.filter((x) => x.to.module !== 'mix'),
        ...l.cables,
        c('glow', 'picture', 'fold', 'picture'),
        c('grid', 'points', 'fold', 'grid'),
        c('fold', 'picture', 'mix', 'in1'),
        c('form-drift', 'out', 'wave', 'circle', 0.7),
        c('form-drift', 'out', 'wave', 'radial', -0.4),
        c('size-drift', 'out', 'wave', 'radius', -0.6),
        c('size-drift', 'out', 'wave', 'amplitude', 0.2),
        c('angle-drift', 'out', 'wave', 'rotation', 0.2),
        c('path-x', 'out', 'wave', 'x', 0.8),
        c('path-y', 'out', 'wave', 'y', 0.7),
      ],
      macros: [
        macro('change', 'Change', 0.8, [
          { module: 'form-drift', knob: 'amount', from: 0, to: 1 },
          { module: 'size-drift', knob: 'amount', from: 0, to: 1 },
          { module: 'angle-drift', knob: 'amount', from: 0, to: 1 },
        ]),
        macro('travel', 'Travel', 0.8, [
          { module: 'path-x', knob: 'amount', from: 0, to: 1 },
          { module: 'path-y', knob: 'amount', from: 0, to: 1 },
        ]),
        macro('spin', 'Spin', 0.25, [{ module: 'fold', knob: 'gridSpin', from: 0, to: -1 }]),
        macro('melt', 'Melt', 0.5, [{ module: 'fold', knob: 'melt', from: 0, to: 1 }]),
      ],
    };
  }),
  // The same recipe on a single star: a Shape in the middle whose sides, size, width and angle drift, so every cell is a
  // kaleidoscope of one ever-changing star.
  rack('star-kaleido', 'Star kaleidoscope', 'One star in the middle, never the same shape, in every cell of a slowly turning honeycomb.', (c) => {
    const l = loop(c, [m('warp', 'warp', { amount: 0.5, zoom: 0.05 }), keep(0.12, 0.13)]);
    return {
      modules: [
        ...io(),
        ...l.modules,
        m('star', 'shape', { kind: 1, sides: 6, size: 0.35, width: 0.15, softness: 0.2, hue: 0.8, saturation: 0.6, brightness: 1.6 }),
        m('glow', 'glow', { glow: 0.5 }),
        m('grid', 'grid', { lattice: 1, count: 36, fit: 1, cover: 1.15, jitter: 0.1, return: 3 }),
        m('fold', 'fold', { mode: 1, count: 8, amount: 1, centre: 0.85, vary: 0.25, reach: 2.5, lift: 0.4, edge: 0.02, gridSpin: 0.3, breathe: 0.6, melt: 0.4 }),
        m('sides-drift', 'random', { rate: 0.4, shape: 0, amount: 0.8 }),
        m('size-drift', 'random', { rate: 0.7, shape: 0.1, amount: 0.8 }),
        m('turn-drift', 'random', { rate: 0.2, shape: 0, amount: 0.8 }),
        m('pulse', 'envelope', { attack: 0.05, release: 0.4 }),
        // The star travelling round the screen (a Transform moved by two slow Randoms).
        m('travel', 'transform', {}),
        m('path-x', 'random', { rate: 0.05, shape: 0, amount: 0.8 }),
        m('path-y', 'random', { rate: 0.035, shape: 0, amount: 0.8 }),
      ],
      cables: [
        ...l.cables,
        c('star', 'picture', 'travel', 'in'),
        c('travel', 'out', 'glow', 'picture'),
        c('path-x', 'out', 'travel', 'x', 0.8),
        c('path-y', 'out', 'travel', 'y', 0.7),
        c('glow', 'picture', 'fold', 'picture'),
        c('grid', 'points', 'fold', 'grid'),
        c('fold', 'picture', 'mix', 'in1'),
        c('sides-drift', 'out', 'star', 'sides', 0.5),
        c('size-drift', 'out', 'star', 'size', 0.4),
        c('size-drift', 'out', 'star', 'width', -0.4),
        c('turn-drift', 'out', 'star', 'turn', 0.5),
        c('turn-drift', 'out', 'star', 'hue', 0.15),
        // The loudness breathes the star's light.
        c('music', 'loudness', 'pulse', 'in'),
        c('pulse', 'level', 'star', 'brightness', 0.3),
      ],
      macros: [
        macro('change', 'Change', 0.8, [
          { module: 'sides-drift', knob: 'amount', from: 0, to: 1 },
          { module: 'size-drift', knob: 'amount', from: 0, to: 1 },
          { module: 'turn-drift', knob: 'amount', from: 0, to: 1 },
        ]),
        macro('travel', 'Travel', 0.8, [
          { module: 'path-x', knob: 'amount', from: 0, to: 1 },
          { module: 'path-y', knob: 'amount', from: 0, to: 1 },
        ]),
        macro('spin', 'Spin', 0.65, [{ module: 'fold', knob: 'gridSpin', from: -1, to: 1 }]),
        macro('melt', 'Melt', 0.4, [{ module: 'fold', knob: 'melt', from: 0, to: 1 }]),
      ],
    };
  }),
  rack('shapes-grid', 'Shapes on a grid', 'A shape at every point of a spiral grid, swirled by the loop.', (c) => {
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
      // The Grid is laid out from its own knobs (graph.ts reads neither cables nor macros there yet): its points set by hand.
      macros: [
        macro('size', 'Size', 0.3, [{ module: 'dot', knob: 'size', from: 0.02, to: 0.1 }]),
        macro('swirl', 'Swirl', 0.65, [{ module: 'warp', knob: 'turnEdge', from: -1, to: 1 }]),
        macro('trail', 'Trail', 0.5, [{ module: 'keep', knob: 'trails', from: 0.3, to: 0.95 }]),
      ],
    };
  }),
  fire(),
  flameKaleido(),
];

export const EXAMPLES: readonly Example[] = [...BUILT, middled(BUILT.find((e) => e.id === 'example:wandering-kaleido')!)];

/** Fire (0.9.18; the owner, 2026-10-29: "and yes, fire"): the Cratarr device in the Fire colours, its Fluid open at the
 *  floor and top, the light lifting by itself and burning out as it climbs (fire-e's 2 and 3 a second), the music's
 *  shove and flashes held back (the owner wants no flashing in a fire). */
function fire(): Example {
  const settings = { ...structuredClone(DEFAULTS), family: 5, fluid: 0.8, stir: 0.4, flash: 0 } as VisualSettings;
  const patch = toV2(settings);
  const fluid = patch.modules.find((x) => x.type === 'fluid');
  if (fluid) fluid.knobs = { ...fluid.knobs, lift: 2, open: 1, cooling: 3 };
  return { id: 'example:fire', label: 'Fire', line: 'Hot light rises by itself, cools as it climbs and burns out at the top.', patch };
}

/** Flame kaleidoscope (2026-10-30): the "Damn!!" recipe on the flame, the best source for it: bright, compact
 *  and never the same (its evolution at half). The owner's Flame rack (examples-flame.ts) with the flame's trails off,
 *  its picture folded into a honeycomb that covers the screen, every cell on the flame's light. */
function flameKaleido(): Example {
  const base = structuredClone(FLAME_BASE);
  const cable = (from: string, fromPort: string, to: string, toPort: string): CableV2 => ({ id: `flame-kaleido:${from}.${fromPort}>${to}.${toPort}`, from: { module: from, port: fromPort }, to: { module: to, port: toPort }, value: 1, depth: null });
  const patch: PatchV2 = {
    ...base,
    modules: [
      ...base.modules,
      m('grid', 'grid', { lattice: 1, count: 36, fit: 1, cover: 1.15, jitter: 0.1, return: 3 }),
      m('fold', 'fold', { mode: 1, count: 10, amount: 1, centre: 0.85, vary: 0.25, reach: 3, lift: 0.3, edge: 0.02, gridSpin: -0.3, breathe: 0.6, melt: 0.4 }),
      // The flame's picture travelling round the screen (a Transform moved by two slow Randoms).
      m('travel', 'transform', {}),
      m('path-x', 'random', { rate: 0.05, shape: 0, amount: 0.8 }),
      m('path-y', 'random', { rate: 0.035, shape: 0, amount: 0.8 }),
    ],
    cables: [
      ...base.cables,
      cable('show', 'picture', 'travel', 'in'),
      cable('travel', 'out', 'layers', 'in1'),
      { ...cable('path-x', 'out', 'travel', 'x'), value: 0.6 },
      { ...cable('path-y', 'out', 'travel', 'y'), value: 0.5 },
      cable('layers', 'picture', 'fold', 'picture'),
      cable('grid', 'points', 'fold', 'grid'),
      cable('fold', 'picture', 'screen', 'picture'),
    ],
    macros: [
      ...base.macros,
      macro('travel', 'Travel', 0.8, [
        { module: 'path-x', knob: 'amount', from: 0, to: 1 },
        { module: 'path-y', knob: 'amount', from: 0, to: 1 },
      ]),
      macro('spin', 'Spin', 0.65, [{ module: 'fold', knob: 'gridSpin', from: -1, to: 1 }]),
      macro('reach', 'Reach', 0.6, [{ module: 'fold', knob: 'reach', from: 1, to: 4 }]),
      macro('melt', 'Melt', 0.4, [{ module: 'fold', knob: 'melt', from: 0, to: 1 }]),
    ],
    pins: [],
  };
  return { id: 'example:flame-kaleido', label: 'Flame kaleidoscope', line: 'The flame in every cell of a honeycomb that fills the screen, turning and breathing slowly.', patch };
}

export const exampleOf = (id: string): Example | undefined => EXAMPLES.find((x) => x.id === id);
