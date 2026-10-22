// The devices we offer (decision 0057 §6b): nine sets of modules the preset study and other modular tools keep coming
// back to, each a device (a module holding a patch of its own, its ports and macros exposed), opened in the graph like
// Show. Data only: the modules are the declared ones (ports.ts, registry.ts), the engine and the compositor flatten
// them.

import { MIX_MODES, type CableV2, type Device, type DeviceMacro, type ExposedPort } from './ports';
import type { Module } from './patch';

const m = (id: string, type: string, knobs: Record<string, number> = {}): Module => ({ id, type, knobs });
let n = 0;
const c = (from: string, fromPort: string, to: string, toPort: string, value = 1, channel?: number): CableV2 => ({
  id: `c${++n}`,
  from: { module: from, port: fromPort, ...(channel !== undefined ? { channel } : {}) },
  to: { module: to, port: toPort },
  value,
  depth: null,
});
const port = (as: string, label: string, module: string, p: string): ExposedPort => ({ as, label, module, port: p });
const macro = (id: string, label: string, value: number, targets: DeviceMacro['targets']): DeviceMacro => ({ id, label, value, targets });
const OVER = MIX_MODES.indexOf('over');
const ADD = MIX_MODES.indexOf('add');
const LIGHTEN = MIX_MODES.indexOf('lighten');

function device(id: string, label: string, modules: Module[], cables: CableV2[], inputs: ExposedPort[], outputs: ExposedPort[], macros: DeviceMacro[]): Device {
  n = 0;
  return { id, type: 'device', label, knobs: {}, patch: { v: 2, modules, cables, macros: [], pins: [] }, inputs, outputs, macros };
}

/** A picture's loop: the new picture over, last frame's (through `between`) by the brighter of the two. */
function looped(between: Module[], wire: (prev: string) => CableV2[]): { modules: Module[]; cables: CableV2[] } {
  const modules = [m('mix', 'mix', { mode1: OVER, amount1: 1, mode2: LIGHTEN, amount2: 1 }), m('feedback', 'feedback'), ...between];
  const chain = ['feedback', ...between.map((x) => x.id)];
  const cables = [
    ...chain.slice(1).flatMap((id, i) => [c(chain[i], 'picture', id, 'picture')]),
    c(chain[chain.length - 1], 'picture', 'mix', 'in2'),
    c('mix', 'picture', 'feedback', 'picture'),
    ...wire(chain[chain.length - 1]),
  ];
  return { modules, cables };
}
const pictureIn = port('picture', 'Picture', 'mix', 'in1');
const pictureOut = port('picture', 'Picture', 'mix', 'picture');

export const DEVICES: readonly Device[] = [
  (() => {
    // MilkDrop's core motion: last frame zoomed and turned, kept, the new picture over it.
    const l = looped([m('warp', 'warp', { amount: 1, zoom: 0.15, turn: 0.1 }), m('keep', 'keep', { amount: 0.8, trails: 0.6 })], () => []);
    return device('feedback-tunnel', 'Feedback Tunnel', l.modules, l.cables, [pictureIn], [pictureOut], [
      macro('zoom', 'Zoom', 0.3, [{ module: 'warp', knob: 'zoom', from: 0, to: 0.5 }]),
      macro('turn', 'Turn', 0.6, [{ module: 'warp', knob: 'turn', from: -0.5, to: 0.5 }]),
      macro('trail', 'Trail', 0.77, [{ module: 'keep', knob: 'amount', from: 0.3, to: 0.95 }]),
    ]);
  })(),
  // The bass's hits through a beat gate at typical, which holds and falls as an envelope does.
  device(
    'kick',
    'Kick',
    [m('kick', 'gate', { above: 1, hysteresis: 0.15, lift: 0.3, recover: 0.5, release: 0.2, gap: 0, amount: 1 })],
    [],
    // Music's bass hits are cabled in (a band's channel of its hits).
    [port('hits', 'Hits', 'kick', 'in')],
    [port('out', 'Out', 'kick', 'out')],
    [
      macro('sensitivity', 'Sensitivity', 0.5, [{ module: 'kick', knob: 'above', from: 2, to: 0.6 }]),
      macro('punch', 'Punch', 1, [{ module: 'kick', knob: 'amount', from: 0.4, to: 1 }]),
      macro('decay', 'Decay', 0.27, [{ module: 'kick', knob: 'release', from: 0.05, to: 0.6 }]),
    ],
  ),
  // The loudness followed slowly: the study's slow inputs.
  device(
    'breather',
    'Breather',
    [m('breath', 'envelope', { attack: 0.8, release: 2.5 })],
    [],
    // Music's loudness is cabled in.
    [port('in', 'Input', 'breath', 'in')],
    [port('out', 'Out', 'breath', 'out')],
    [
      macro('smoothness', 'Smoothness', 0.33, [
        { module: 'breath', knob: 'attack', from: 0.2, to: 2 },
        { module: 'breath', knob: 'release', from: 0.5, to: 5 },
      ]),
      macro('depth', 'Depth', 1, [{ module: 'breath', knob: 'intensity', from: 0.3, to: 1 }]),
    ],
  ),
  // Motion with time, not the music: a smooth random wander.
  device(
    'wanderer',
    'Wanderer',
    [m('wander', 'random', { rate: 0.1, shape: 0, amount: 1 })],
    [],
    [],
    [port('out', 'Out', 'wander', 'out')],
    [
      macro('speed', 'Speed', 0.17, [{ module: 'wander', knob: 'rate', from: 0.02, to: 0.5 }]),
      macro('range', 'Range', 1, [{ module: 'wander', knob: 'amount', from: 0.2, to: 1 }]),
    ],
  ),
  // The bright parts, blurred, added back: a bright pass first, then the blur, as every bloom does.
  device(
    'bloom',
    'Bloom',
    [m('mix', 'mix', { mode1: OVER, amount1: 1, mode2: ADD, amount2: 0.6 }), m('bright', 'threshold', { threshold: 0.6, softness: 0.1 }), m('blur', 'blur', { smoothing: 0.6 })],
    [c('bright', 'picture', 'blur', 'picture'), c('blur', 'picture', 'mix', 'in2')],
    // One input, to both: the picture as it is, and its bright pass.
    [pictureIn, port('picture', 'Picture', 'bright', 'picture')],
    [pictureOut],
    [
      macro('threshold', 'Threshold', 0.6, [{ module: 'bright', knob: 'threshold', from: 0.9, to: 0.4 }]),
      macro('size', 'Size', 0.5, [{ module: 'blur', knob: 'smoothing', from: 0.2, to: 1 }]),
      macro('strength', 'Strength', 0.5, [{ module: 'mix', knob: 'amount2', from: 0.2, to: 1 }]),
    ],
  ),
  // MilkDrop's waveform: the sound drawn, glowing, in the colours of the picture it is cabled to.
  device(
    'scope',
    'Scope',
    [m('wave', 'waves', { width: 0.01, opacity: 0.9 }), m('glow', 'glow', { glow: 0.5 }), m('tint', 'tint', { tint: 0.8 })],
    // The Waves' sound is Music's by its normal.
    [c('wave', 'picture', 'glow', 'picture'), c('glow', 'picture', 'tint', 'picture')],
    [port('colours', 'Colours from', 'tint', 'by')],
    [port('picture', 'Picture', 'tint', 'picture')],
    [
      macro('height', 'Height', 0.36, [{ module: 'wave', knob: 'amplitude', from: 0.1, to: 0.8 }]),
      macro('glow', 'Glow', 0.5, [{ module: 'glow', knob: 'glow', from: 0.1, to: 0.9 }]),
      macro('colour', 'Colour', 0.8, [{ module: 'tint', knob: 'tint', from: 0, to: 1 }]),
    ],
  ),
  // A shape at every point of a grid.
  device(
    'grid-particles',
    'Grid Particles',
    [m('grid', 'grid', { lattice: 1, count: 24, spacing: 0.25 }), m('dot', 'shapes', { count: 1, spread: 0, sides: 6, radius: 0.06, fill: 0.8, opacity: 0.9 }), m('each', 'instance', { amount: 1 })],
    [c('grid', 'points', 'each', 'points'), c('dot', 'picture', 'each', 'picture')],
    [],
    [port('picture', 'Picture', 'each', 'picture')],
    [
      macro('count', 'Count', 0.33, [{ module: 'grid', knob: 'count', from: 4, to: 64 }]),
      macro('spacing', 'Spacing', 0.5, [{ module: 'grid', knob: 'spacing', from: 0.1, to: 0.4 }]),
      macro('pulse', 'Size', 0.33, [{ module: 'dot', knob: 'radius', from: 0.03, to: 0.12 }]),
    ],
  ),
  (() => {
    // An echo of the picture trailing it: Hydra's Echo in a picture loop (the effect modules' own types follow).
    const l = looped([m('echo', 'hydra-engine', { effect: 1, amount: 0.7, k1: 0.5, k2: 0.5 })], () => []);
    return device('echo-trails', 'Echo Trails', l.modules, l.cables, [pictureIn], [pictureOut], [
      macro('length', 'Length', 0.57, [{ module: 'echo', knob: 'amount', from: 0.3, to: 1 }]),
      macro('drift', 'Drift', 0.5, [{ module: 'echo', knob: 'k1', from: 0.2, to: 0.8 }]),
    ]);
  })(),
  (() => {
    // MilkDrop's self-alike feedback: last frame folded, zoomed in and turned, kept: a spiral of itself.
    const l = looped(
      [m('fold', 'fold', { mode: 1, count: 6, turn: 0, amount: 1 }), m('warp', 'warp', { amount: 1, zoom: 0.1, turn: 0.1 }), m('keep', 'keep', { amount: 0.7, trails: 0.6 })],
      () => [],
    );
    return device('fractal-spiral', 'Fractal Spiral', l.modules, l.cables, [pictureIn], [pictureOut], [
      macro('folds', 'Folds', 0.33, [{ module: 'fold', knob: 'count', from: 3, to: 12 }]),
      macro('turn', 'Turn', 0.67, [{ module: 'warp', knob: 'turn', from: -0.3, to: 0.3 }]),
      macro('depth', 'Depth', 0.6, [{ module: 'keep', knob: 'amount', from: 0.4, to: 0.9 }]),
    ]);
  })(),
];
