// What each knob means, for the aspect dice (prototype, 2026-10-30; the owner: "all nodes should select by what they
// mean"): Spatial, Motion, Colours and Lightness each roll every knob of their aspect in any module, a source's and the
// loop's as much as a Grid's or a Fold's (before, Spatial was a list of module types and never reached a source).
//
// Declared here by knob id, one table, so a renamed or new knob shows as unclassified in tests/knob-aspects.test.ts
// instead of silently falling out of a scope; a module's own declaration (`aspect` on its knob) wins where there is one,
// and BY_MODULE overrides an id that means something else in one module. Knobs of none of the four (how much, which
// mode, the dynamics of followers and envelopes) are `null`: the module and category dice reach them, the aspects don't.

export type KnobAspect = 'spatial' | 'motion' | 'colour' | 'light';

/** Each knob id's aspect, `null` for none of the four. */
const BY_ID: Record<string, KnobAspect | null> = {
  // Where things are and how space is laid out: place, size, shape of the layout, turning to an angle, zoom, symmetry.
  across: 'spatial', x: 'spatial', y: 'spatial', up: 'spatial', centre: 'spatial', centreX: 'spatial', centreY: 'spatial',
  circle: 'spatial', radius: 'spatial', radial: 'spatial', length: 'spatial', width: 'spatial', size: 'spatial', scale: 'spatial',
  h: 'spatial', amplitude: 'spatial', thickness: 'spatial', zoom: 'spatial', zoomEdge: 'spatial', rotation: 'spatial',
  rotate: 'spatial', turn: 'spatial', turnEdge: 'spatial', twist: 'spatial', spacing: 'spatial', count: 'spatial',
  lattice: 'spatial', jitter: 'spatial', fit: 'spatial', cover: 'spatial', reach: 'spatial', distance: 'spatial',
  repeat: 'spatial', round: 'spatial', roundCount: 'spatial', rings: 'spatial', ringCount: 'spatial', ring: 'spatial',
  petals: 'spatial', petalCount: 'spatial', petalTurn: 'spatial', stretch: 'spatial', wobbleSize: 'spatial',
  turbulenceSize: 'spatial', horizon: 'spatial', sides: 'spatial', kind: 'spatial', edge: 'spatial', align: 'spatial',
  vary: 'spatial', melt: 'spatial', offset: 'spatial', mirror: 'spatial', symmetry: 'spatial', clusters: 'spatial',
  detail: 'spatial', roughness: 'spatial', spread: 'spatial', open: 'spatial', gap: 'spatial', bars: 'spatial',
  waves: 'spatial', h1x: 'spatial', h1y: 'spatial', h2x: 'spatial', h2y: 'spatial',
  // How fast and how restlessly things move: speeds, rates, drift, flow, turning as a speed, trails' persistence.
  spin: 'motion', speed: 'motion', rate: 'motion', drift: 'motion', driftX: 'motion', driftY: 'motion', ringSpeed: 'motion',
  turbulenceSpeed: 'motion', gridSpin: 'motion', breathe: 'motion', orbit: 'motion', flow: 'motion', stir: 'motion',
  swirl: 'motion', glide: 'motion', wobble: 'motion', turbulence: 'motion', return: 'motion', trails: 'motion',
  fall: 'motion', cooling: 'motion', course: 'motion', beats: 'motion', follow: 'motion', variety: 'motion', calm: 'motion',
  // Colour.
  hue: 'colour', hueSpread: 'colour', saturation: 'colour', tint: 'colour',
  // How light the picture is.
  brightness: 'light', exposure: 'light', glow: 'light', lift: 'light', contrast: 'light', opacity: 'light',
  intensity: 'light', level: 'light', fill: 'light',
  // None of the four: how much, which mode, dynamics, references, recording.
  amount: null, mode: null, effect: null, invert: null, gain: null, depth: null, attack: null, decay: null, sustain: null,
  release: null, hold: null, threshold: null, knee: null, ratio: null, makeup: null, hysteresis: null, above: null,
  full: null, range: null, recover: null, reference: null, retrigger: null, record: null, overdub: null, window: null,
  scope: null, smoothing: null, softness: null, taper: null, accent: null, additive: null, perSeed: null, k1: null, k2: null,
  shape: null,
  // The Flame's own (the Cratarr device): its light and colour, its lens' shape, how fast it evolves.
  lightness: 'light', lights: 'light', presence: 'light', lighting: 'light', haze: 'light',
  vividness: 'colour', purity: 'colour', cycle: 'colour',
  lensSides: 'spatial', lensDepth: 'spatial', tiles: 'spatial', power: 'spatial', swell: 'spatial',
  evolution: 'motion', continuity: 'motion', way: 'motion', lookWalk: 'motion', behaviourWalk: 'motion',
  beatRatio: 'motion', barRatio: 'motion', phraseRatio: 'motion', phraseBars: 'motion',
  // Structure and plumbing: which form, which family, the frame's quality, steps, splits, bands, a mix's inputs.
  form: null, warp: null, final: null, family: null, fluid: null, facets: null, tunnel: null, dive: null, quality: null,
  fps: null, flash: null, wild: null, loose: null, effects: null, steps: null,
};

/** Numbered knobs (a Mix's mode1…, a Sequencer's step1…, a Volume's band1…, a Filter bank's split1…): their stem's. */
const STEMS: Record<string, KnobAspect | null> = { mode: null, amount: null, step: null, band: null, split: null };

/** Where an id means something else in one module type. */
const BY_MODULE: Record<string, Record<string, KnobAspect | null>> = {
  // A Fold's Centre is where its cells look (place), a Grid's Count how dense the layout is: both already spatial.
  // A Keep's Amount is how much of the last picture stays: how long things linger, so motion.
  keep: { amount: 'motion' },
  // Followers' and Shapers' Speed is how fast they answer the music: response, not the picture's motion.
  envelope: { speed: null, rate: null },
  shaper: { speed: null, rate: null },
};

/** A knob's aspect: its module's declaration, else this table (module first), else unclassified (`undefined`). */
export function aspectOf(moduleType: string, knobId: string, declared?: KnobAspect | null): KnobAspect | null | undefined {
  if (declared !== undefined) return declared;
  const own = BY_MODULE[moduleType];
  if (own && knobId in own) return own[knobId];
  // A polyphonic knob's channel (`hold.3`) is its knob's.
  const id = knobId.includes('.') ? knobId.slice(0, knobId.indexOf('.')) : knobId;
  if (id in BY_ID) return BY_ID[id];
  const stem = id.replace(/\d+$/, '');
  return stem in STEMS ? STEMS[stem] : undefined;
}
