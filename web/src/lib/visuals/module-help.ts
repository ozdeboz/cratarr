// What each module type (decision 0057) and each of its ports does, a line each: for the graph's node and the long-hold
// help (held.svelte.ts), never shown otherwise. A knob that already has its text in help.ts (a setting's, a band's
// time, a Music signal's) points to it rather than saying it again; only what help.ts lacks is written here. A port
// whose label says it all has no line (PLAIN). Every declared module type and port has one or the other: tests/
// module-help.test.ts holds a new module to that. Pure.

import { HELP } from './help';
import { RENAMED } from './modulators/modulators';
import { SETTING_KNOB } from './patch';
import { SHOW_PARTS, TRIGGER_SIGNALS } from './ports';
import { BAND_SIGNALS, TRACK_SIGNALS } from './routes';

/** What a module does to the picture or the values, a line each, by its type. */
export const moduleHelp: Record<string, string> = {
  // Sources.
  flame: 'The fractal flame: six parts, one for each band of the sound, drawn as light.',
  shapes: 'Polygons and stars on a ring, outlined or filled, each spinning as the ring turns; their sides and depth morph.',
  irregular: 'A heart, the Cratarr mark, an arrow or another shape on a ring, outlined or filled, in white light.',
  noise: 'Clouds of noise over the whole picture, in white light; at its finest scale, moving, film grain.',
  gradient: 'Light by place: out from a centre, round it, across or up, blended; into a Displace, a motion field.',
  waves: 'The sound’s waveform drawn as a line, a circle or spokes (without live sound, the bands’ levels).',

  // Effects.
  show: 'The flame’s output stage: its light turned into a picture, with size, glow, saturation, smoothing, haze and waves.',
  tonemap: 'Tone map: turns the flame’s light into a picture, how bright set by Glow.',
  curve: 'A tone curve: lifts the faint areas into view or sinks them to black.',
  saturate: 'Makes the colours richer or paler.',
  blur: 'Softens sparse, grainy areas into a blur; dense areas stay sharp.',
  bend: 'Bends the picture along slow giant waves.',
  fold: 'Folds the picture by a symmetry: mosaic, kaleidoscope, mirror or pixelate.',
  instance: 'Draws its picture once at each point.',
  shape: 'One shape in the middle: a circle, polygon, star, ring or line.',
  'kaleidoscope-engine': 'One of the Kaleidoscope’s effects over the picture: Kaleid, Repeat, Rotate, Scroll, Pixelate or Magnify.',
  'eyedo-engine': 'One of EYEDO’s effects over the picture: Pulse, Burst, Halo, Rings or Interference.',
  'hydra-engine': 'One of Hydra’s effects over the picture: Echo, Ripple, Stream, Colorama, Hue, Saturate or Posterize.',
  warp: 'Moves the last frame’s picture before it is kept, alike everywhere or by place (MilkDrop’s grid).',
  keep: 'Keeps part of the last frame’s picture, so movement leaves trails (MilkDrop’s decay).',
  'fluid-loop': 'Carries the picture along a fluid flow the music stirs, like smoke.',
  fluid: 'Makes the flame’s light flow like smoke, stirred by the music.',
  tunnel: 'Streams the flame’s light outward from the middle for ever, as if flying into it.',
  dive: 'Zooms for ever into a small, turned copy of the flame inside the flame.',
  trails: 'How long the flame’s light lingers behind it as it moves.',
  'loop-effect': 'An effect on the flame’s light inside its own loop, so it builds up into new shapes.',

  // Mixers.
  mix: 'Mixes up to four pictures, each by its blend mode and amount, the first under the rest.',
  feedback: 'The picture its input had last frame: the only way to close a picture loop.',

  // Modulators.
  envelope: 'An envelope follower: on Music’s loudness, it scales every cable’s depth unless unplugged.',
  clock: 'The tempo’s clock: a turn every so many beats, and beat, bar and phrase triggers.',
  accent: 'Splits a value into a slow course and the swing about it, and keeps Accent’s share of the swing.',
  vca: 'A VCA: its input times its gain, the one way one signal multiplies another.',
  merge: 'Makes one channel of a polyphonic cable’s several (VCV’s poly to mono).',
  compressor: 'A sidechain compressor on values: turns its input down as the sidechain goes over the threshold.',
  ducker: 'A sidechain ducker: lowers its input while the sidechain stands over the threshold, for counts like looks a minute.',
  glow: 'A soft halo around the picture’s light.',
  tint: 'The picture in the colours of another, by its brightness.',
  mask: 'The picture let through only where another is bright.',
  displace: 'The picture pushed about by another.',
  threshold: 'Only the picture’s bright parts, the rest black: a luma key.',
  transform: 'Moves, scales and turns the picture.',
  lens: 'Points, light or a picture through a lens: a warp (Spherical, Swirl …) or a shape (Heart, Cratarr, Star …).',
  adsr: 'An ADSR envelope on each gate or trigger, one a channel.',
  sequencer: 'A step sequencer: up to sixteen values, one a clock step.',
  shaper: 'A value through a curve, as a curved cable shapes it.',
  dice: 'The panel’s dice as a module: a trigger randomizes every knob in its scope that is not pinned, from its seed.',
  looper: 'Records a value over a number of bars and loops it in time with the Clock.',
  grid: 'Points on a lattice, with each point’s values as channels.',
  field: 'A flow that swirls and changes by itself: for Streamlines to trace.',
  streamlines: 'Lines traced through a flow from seed points, drawn anew every frame.',
  push: 'Pushes a picture along a flow, so it smears and swirls the way the flow goes.',
  lfo: 'A low-frequency oscillator, sine to triangle; in time with a clock when one is plugged in.',
  random: 'Random values at a rate, from smooth glides to stepped jumps.',
  hold: 'Sample and hold: takes its input at a rate and holds it.',
  volume: 'The bands’ levels added up, each by its weight (MilkDrop’s vol).',
  fast: 'An envelope follower with a short attack and release: follows its input closely.',
  average: 'A moving average of its input over its window.',
  peak: 'Peak hold: the highest input lately, falling back over its Fall.',
  mean: 'The running mean of its input over its horizon, from its first value on.',
  gate: 'A comparator with hysteresis: gives 1 while its input is above the threshold.',
  counter: 'Counts each time its input rises past half way, stepping by 1/Length and wrapping at 1.',

  // In / Out.
  music: 'The playing music: its sound, the whole track’s measures and each band’s, as values and triggers.',
  filterbank: 'A crossover: splits the sound into six bands, with each band’s level.',
  meter: 'A detector: how busy a picture is, as motion, churn, zoom pumping and new looks.',
  screen: 'Draws what reaches it, through the flash guard.',
  frame: 'The settings that act on the whole picture, and the device’s own.',

  // Not a declared type: a group of modules (0057 §6).
  device: 'A group of modules shown as one, with its macro knobs; open it to see and change what is inside.',
};

// Lines several types share.
const AMOUNT_EFFECT = 'How much of the effect shows: none at 0, all of it at 1.';
const AMOUNT_OUT = 'Scales what it gives: nothing at 0, all of it at 1.';
const FULL = 'The input that gives a full output (1 is typical): higher needs a stronger signal.';
const LIGHT = 'The flame’s light, before Tone map turns it into a picture.';
const HUE = 'The colour, round the colour wheel.';
const SATURATION = 'How vivid the colours are: grey at 0.';
const ACROSS = 'Moves it left or right.';
const UP = 'Moves it up or down.';
const SPIN = 'How fast it turns; below 0 the other way.';
const ATTACK = 'How quickly it rises to a higher input (seconds).';
const RELEASE = 'How quickly it falls to a lower input (seconds).';
/** Lines for the inputs every level knob and every step knob has (ports.ts): `<knob>:gain`, `<knob>:next`. */
const SUFFIX: Record<string, string> = {
  gain: 'How much of it gets through, 1 while unplugged: a level’s value, a swing’s movement; a time divided by it.',
  next: 'A trigger here moves its knob on to the next step, at its module’s neutral.',
};

const numbered = (n: number, id: (i: number) => string, line: (i: number) => string) => Object.fromEntries(Array.from({ length: n }, (_, i) => [id(i + 1), line(i + 1)]));

/** Where both shape sources lay their shapes and how they stroke them (sources/shapes.ts). */
const SHAPE_RING = {
  count: 'How many shapes on the ring; none at 0.',
  radius: 'How big each shape is.',
  thickness: 'How wide the outline is.',
  fill: 'From an outline (0) to filled (1).',
  spread: 'How far out the ring is drawn.',
  x: ACROSS,
  y: UP,
  orbit: 'How fast the shapes travel round the ring; below 0 the other way.',
  rotation: 'Turns every shape.',
  spin: SPIN,
  twist: 'Turns each shape a little more than the one before.',
  taper: 'Each shape smaller (above 0) or larger (below 0) than the one before.',
  additive: 'From covering what is under it (0) to adding its light to it (1).',
};

/** A port's or knob's own line, where its label does not say it all and help.ts has none. */
export const portHelp: Record<string, Record<string, string>> = {
  flame: { light: 'Its light: in, as it was last frame; out, as drawn now, before Tone map.', points: 'Where its parts are: where the Fluid pushes, unless cabled otherwise.', x: 'Its centre, across.', y: 'Its centre, up.', size: 'How big it is drawn.', coverage: 'How much of the screen it covers: for exposure.', fit: 'How far it frames itself: centred and sized to the screen at 1, where its points fall at 0.' },
  shapes: {
    ...SHAPE_RING,
    sides: 'Corners a shape; between whole numbers it morphs, near a circle at many.',
    depth: 'From the polygon (0) to a star whose points go deeper.',
    corners: 'Each shape’s corners as points, for Instance: a smaller copy at each makes a corner fractal.',
    hue: HUE,
    hueSpread: 'How far the colour moves round the wheel from shape to shape.',
    saturation: SATURATION,
    additive: 'From covering what is under it (0) to adding its light to it (1).',
  },
  gradient: {
    amount: 'How bright it is; none at 0.',
    distance: 'Light growing out from the centre to the edge; below 0 the other way.',
    round: 'Light round the centre, in petals; below 0 the other way.',
    roundCount: 'How many petals round the centre; between two whole numbers the next one fades in.',
    across: 'Light growing from left to right; below 0 the other way.',
    up: 'Light growing from the bottom up; below 0 the other way.',
    centreX: 'Where its centre is, across: the middle at 0.',
    centreY: 'Where its centre is, up or down: the middle at 0.',
    turn: 'Turns it about its centre.',
    repeat: 'Bands of it, this many; at 0 the plain field, the bands fading in up to 1.',
    speed: 'How fast the bands move, a second; below 0 the other way.',
  },
  noise: {
    amount: 'How bright it is; none at 0.',
    scale: 'From the finest, a speck a pixel or two (grain), to clouds the size of the picture.',
    detail: 'Layers of finer noise over it, 1 to 6; between whole numbers the last fades in.',
    speed: 'How fast it changes, by its own size: at the finest scale a new pattern every frame.',
    contrast: 'From soft greys (0) through as it is (50%) to hard black and white.',
  },
  irregular: {
    ...SHAPE_RING,
    shape: 'Which shape of the set: Heart, Cratarr, Ring, Cross, Arrow, Crescent, Drop, Polar or Handkerchief.',
  },
  waves: {
    audio: 'The sound to draw: Music’s unless unplugged.',
    width: 'How wide the line is; none at 0.',
    amplitude: 'How far the wave swings from its line.',
    circle: 'From a line (0) to a circle (1).',
    radial: 'Draws it as spokes out from the circle (1).',
    radius: 'How big the circle is.',
    length: 'How much of the screen’s width the line spans.',
    x: ACROSS,
    y: UP,
    rotation: 'Turns the wave.',
    spin: SPIN,
    smoothing: 'Smooths the line: jagged at 0, rounded at 1.',
    hue: HUE,
    hueSpread: 'How far the colour moves round the wheel along the line.',
    saturation: SATURATION,
  },
  tonemap: { light: LIGHT, level: 'The whole picture’s brightness, as a fader: 1 as set; the pause dims it through its gain.', exposure: 'Keeps the picture as bright however much of the screen the flame covers: at 1 fully, at 0 not at all.' },
  fluid: {
    light: LIGHT,
    fluid: 'How much the picture flows like smoke: the most it reaches; cables into its gain take it lower.',
    lift: 'How hard the light rises by itself, as hot air does: the densest the most. At 0 it goes only where pushed.',
    open: 'From a sealed box (0) to an open floor and top (1): air comes in below and out above. The sides stay walls.',
    cooling: 'How fast the light burns out as it climbs (from 40% of the height, and along the sides): flames end as fire does.',
    stir: 'How much of what its cables give pushes the fluid.',
    stirBy: 'A Field stirring it everywhere; the cable’s amount sets how hard.',
    flow: 'How much of what its cables give makes the fluid flow.',
    swirl: 'How much of what its cables give sets the fluid whirling.',
    facets: 'How much the fluid breaks into facets: the most it reaches; cables into its gain take it lower.',
    at: 'Where its pushes land: the flame’s parts unless cabled (a Grid’s points, an edge’s).',
    field: 'Its flow field, where it moves things and how fast: into Show’s Bend, Stream or a Displace.',
  },
  tunnel: { light: LIGHT, tunnel: 'How fast the picture streams outward for ever: the most it reaches; cables into its gain take it lower.' },
  dive: { light: LIGHT, dive: 'How fast the view dives into the flame for ever: the most it reaches; cables into its gain take it lower.' },
  trails: { light: LIGHT },
  bend: { field: 'The flow it reads the picture along, as the fluid moves it: the Fluid’s unless cabled.' },
  'loop-effect': { light: LIGHT, field: 'The flow Stream reads the picture along: the Fluid’s unless cabled.', amount: 'How strongly the loop’s effect acts: the most it reaches; cables into its gain take it lower.' },
  'kaleidoscope-engine': { amount: 'How strongly its effect shows: the most it reaches; cables into its gain take it lower.' },
  'eyedo-engine': { amount: 'How strongly its effect shows: the most it reaches; cables into its gain take it lower.' },
  'hydra-engine': { field: 'The flow Stream reads the picture along: the Fluid’s unless cabled.', amount: 'How strongly its effect shows: the most it reaches; cables into its gain take it lower.' },
  blur: { smoothing: 'How wide sparse areas blur, up to twelve pixels: the most it reaches; cables into its gain take it lower.' },
  curve: { haze: 'Lifts the faint mist into view (up) or sinks it to black (down): the most either way; cables into its gain take it toward 0.' },
  fold: {
    grid: 'A Grid’s points set the symmetry; without one, its own centre and Count.',
    mode: 'Mosaic, kaleidoscope, mirror, pixelate or cells.',
    zoom: 'Cells: how much nearer each cell shows its own part of the picture.',
    edge: 'With a grid: how wide the dark seams between the cells are.',
    centre: 'With a grid: where each cell looks, at its own point (0) or all at the picture’s middle (1), so every cell shows something.',
    reach: 'Kaleidoscope and mirror: how far each cell reaches into the picture; above 1 it takes in more than its own cell.',
    lift: 'Raises a dim picture towards a steady brightness, as a camera’s exposure does: up to eight times at 1.',
    vary: 'With Centre: each cell looks a little off the light, in a direction of its own, so each shows its own variation.',
    gridSpin: 'With a grid: turns the whole grid slowly, up to a turn in two minutes; below 0 the other way.',
    breathe: 'With a grid: zooms the grid slowly in and back, a breath every 16 seconds, up to a quarter nearer.',
    melt: 'With a grid: blends each cell into its neighbours across their borders instead of a hard seam.',
    window2: 'With a grid: another picture, shown in some of the cells.',
    window3: 'With a grid: a third picture, shown in some of the cells.',
    count: 'How many times it repeats.',
    turn: 'Turns the fold; below 0 the other way.',
    amount: AMOUNT_EFFECT,
  },
  instance: {
    points: 'Where to draw it: a Grid’s points.',
    picture: 'What to draw at each point.',
    amount: AMOUNT_EFFECT,
    align: 'How far each copy turns the way the flow goes.',
    stretch: 'How much longer each copy is drawn as the flow goes faster.',
    field: 'The flow that turns and stretches the copies: the Fluid’s unless a Field is cabled.',
    along: 'A picture whose outlines the flow runs along: the flame, a Shape, the cover.',
    follow: 'How strongly the flow runs along Along’s outlines; below 0 the other way round.',
    reach: 'How far from Along’s edges the flow is felt (its picture blurred that far).',
  },
  shape: {
    kind: 'Circle, polygon, star, ring or line.',
    sides: 'A polygon’s sides, a star’s points.',
    size: 'How big it is.',
    turn: 'Turns it; below 0 the other way.',
    width: 'A ring’s or line’s thickness; how deep a star’s points go.',
    softness: 'How soft its edge is.',
    hue: 'Its colour.',
    saturation: 'How strong its colour is; 0 is white.',
    brightness: 'How bright it is.',
  },
  warp: {
    amount: AMOUNT_EFFECT,
    zoom: 'Zooms the kept picture in (above 0) or out (below 0), a second.',
    turn: 'Turns the kept picture, a second; below 0 the other way.',
    driftX: 'Moves the kept picture sideways, a second.',
    driftY: 'Moves the kept picture up or down, a second.',
    stretch: 'Stretches the kept picture wider (above 0) or taller (below 0), a second.',
    wobble: 'Bends the kept picture as water does.',
    wobbleSize: 'How fine the wobble’s ripples are.',
    centreX: 'Where it zooms and turns about, across: the middle at 0.',
    centreY: 'Where it zooms and turns about, up or down: the middle at 0.',
    zoomEdge: 'Zooms more towards the edge than at the centre (below 0 the other way): a tunnel, or a bulge.',
    turnEdge: 'Turns more towards the edge than at the centre (below 0 the other way): a swirl.',
    petals: 'Zooms in and out in petals round the centre.',
    petalCount: 'How many petals; between two whole numbers the next one fades in.',
    petalTurn: 'Turns the petals round, a second; below 0 the other way.',
    rings: 'Zooms in and out in rings running out from the centre.',
    ringCount: 'How many rings from the centre to the edge.',
    ringSpeed: 'How fast the rings run out; below 0 they run in.',
    turbulence: 'MilkDrop’s own warp: slow waves that bend the picture as it flows.',
    turbulenceSpeed: 'How fast the turbulence changes.',
    turbulenceSize: 'How large its waves are.',
  },
  keep: {
    amount: 'How much of the last picture stays: none at 0, as without a loop.',
    trails: 'How long the trails last: a tenth of a second at 0, six seconds at 1.',
  },
  'fluid-loop': {
    light: 'Light, a picture or points, carried along its flow; out the same.',
    at: 'Where its pushes land: the flame’s parts unless cabled (a Grid’s points, an edge’s).',
    field: 'Its flow field, where it moves things and how fast: into Show’s Bend, Stream or a Displace.',
    amount: 'How far the picture moves with the flow: not at all at 0.',
    stir: 'How hard the music pushes the flow.',
    flow: 'How long the flow’s motion lasts.',
    swirl: 'How strongly its small whirls are kept.',
  },
  mix: {
    in1: 'The bottom picture; the others go over it in order.',
    ...numbered(4, (i) => `mode${i}`, (i) => `How picture ${i} blends: add, screen, over, multiply, window, lighten or subtract.`),
    ...numbered(4, (i) => `amount${i}`, (i) => `How much of picture ${i} shows.`),
  },
  feedback: { picture: 'In: the picture to hold back a frame; out: that picture as it was last frame.' },
  envelope: {
    in: 'What it follows: Music’s loudness unless another cable is plugged in.',
    attack: 'How quickly it follows a rise (seconds).',
    release: 'How quickly it follows a fall (seconds).',
    level: 'What it follows, as far as Intensity lets it count.',
    out: 'The level while the music plays, else 0: every cable’s depth unless unplugged.',
  },
  clock: {
    tempo: 'Music’s tempo unless another cable is plugged in.',
    beats: 'Beats a turn: how many beats it takes to go once round.',
    phase: 'Where it is in its turn, from 0 to 1.',
    rate: 'How fast it turns now.',
    beatRatio: 'The Beat output: every beat, or the beats divided or multiplied (÷4 to ×4).',
    barRatio: 'The Bar output: every bar, or the bars divided or multiplied (÷4 to ×4).',
    phraseRatio: 'The Phrase output: every phrase, or the phrases divided or multiplied (÷4 to ×4).',
    phraseBars: 'How many bars make a phrase.',
  },
  accent: {
    in: 'The value to split.',
    accent: 'How much of the swing about the course it keeps: none at 0, twice it at 2.',
    course: 'How slowly the course follows (seconds): the swing about it is what Accent keeps.',
  },
  vca: { gain: 'The gain: 0 shuts the input off, 1 lets it through as it is, 2 doubles it.', cv: 'The gain as a signal (CV), cabled in.' },
  merge: { mode: 'Sum, mean, largest or geometric mean of the channels.' },
  ducker: {
    key: 'The sidechain: the count it watches.',
    in: 'The signal it lowers; 1 when unplugged.',
    threshold: 'The count above which it starts to duck.',
    depth: 'How far it lowers at most (dB).',
    attack: 'How quickly it ducks (seconds).',
    hold: 'How long it stays down once the count is back under (seconds).',
    release: 'How quickly it comes back up (seconds).',
    hysteresis: 'How far under the threshold the count must fall before it lets go, as a share.',
    out: 'Its input, ducked.',
  },
  glow: { glow: 'How much halo.' },
  tint: { tint: 'How much of the other picture’s colours.', by: 'The picture whose colours it takes.' },
  mask: { amount: 'How strongly it masks.', invert: 'Let through where the other is dark instead.', by: 'The picture that decides where.' },
  displace: {
    amount: 'How far it pushes.',
    scale: 'How fine the pushing is.',
    mode: 'By the other’s brightness, or by its red and green.',
    by: 'The picture that pushes.',
    zoom: 'Zooms it about the middle by the other’s brightness: in where light, out where dark.',
    turn: 'Turns it about the middle by the other’s brightness: one way where light, the other where dark.',
  },
  threshold: { threshold: 'The brightness under which it goes black.', softness: 'How soft the edge is.' },
  transform: {
    in: 'Light or a picture; out the same, moved.',
    x: 'How far across it moves.',
    y: 'How far up it moves.',
    scale: 'How big: 1 as it is.',
    rotate: 'How far it turns: a whole turn at ±1.',
  },
  lens: {
    in: 'Points, light or a picture to send through the lens; out the same.',
    kind: 'Which lens: a warp of the whole, or a shape the points are sent into.',
    amount: 'How much of the lens: none at 0.',
    sides: 'Polygon’s and Star’s sides.',
    depth: 'Star’s depth: 0 a polygon.',
  },
  compressor: {
    in: 'The signal it compresses.',
    key: 'The sidechain: the signal whose level sets how far it compresses.',
    threshold: 'The sidechain level above which it starts to compress.',
    ratio: 'How hard it compresses above the threshold: at 4, a quarter of the excess gets through.',
    knee: 'How gently it starts around the threshold: a hard knee at 0.',
    attack: 'How quickly it clamps down (seconds).',
    release: 'How quickly it lets go (seconds).',
    makeup: 'Makeup gain, after compressing.',
    out: 'The compressed signal.',
    reduction: 'The gain reduction (1: none), to duck anything else.',
  },
  adsr: {
    gate: 'Starts an envelope on each trigger, one a channel.',
    attack: 'Attack: the time to rise to the peak (seconds).',
    decay: 'Decay: the time to fall from the peak to Sustain (seconds).',
    sustain: 'Sustain: the level held while the gate stays open.',
    release: 'Release: the time to fall to 0 once the gate closes (seconds).',
    retrigger: 'A trigger while one rings starts from the higher of the two (0) or from where it is (1).',
    reference: 'How it reads a strike’s strength: against this track’s typical, lately’s, or as it is.',
    amount: 'The peak’s height.',
    out: 'One envelope a channel.',
  },
  shaper: {
    in: 'The value to shape, a channel a band.',
    h: 'The curve’s height at its end.',
    h1x: 'The first handle, across.',
    h1y: 'The first handle’s height.',
    h2x: 'The second handle, across.',
    h2y: 'The second handle’s height.',
    out: 'The curve’s height at the value, a channel a band.',
  },
  dice: {
    trigger: 'Randomizes on each trigger.',
    seed: 'Where its randomness starts: the same seed, the same values (a track’s seed, the same look).',
    scope: 'What it rolls: everything, one category, the cables, the response, the macros, the colours, space or lightness.',
    done: 'A trigger as it randomizes.',
    reach: 'How far each knob moves toward its roll: all the way at 100 %, not at all at 0.',
    glide: 'Seconds each roll morphs in as the music plays, with no undo step; 0, as a new track’s look does.',
  },
  sequencer: {
    clock: 'Moves on a step; the Clock’s beat unless unplugged.',
    reset: 'Back to the first step; on the Clock’s bar unless unplugged.',
    length: 'How many steps before it starts over.',
    out: 'The value of the step it is on.',
    gate: 'A trigger on each step.',
  },
  looper: {
    in: 'The value it records.',
    clock: 'Keeps the loop in time; the Clock’s bar unless unplugged.',
    bars: 'How many bars the loop lasts.',
    record: 'On: records its input over the loop.',
    overdub: 'On: adds its input to what is recorded.',
    out: 'The loop as recorded.',
  },
  field: {
    scale: 'How big its swirls are.',
    speed: 'How fast it changes.',
    waves: 'Each swirl size’s strength, biggest first; a band’s cable gives each its band.',
    range: 'How far apart the biggest and the smallest swirls are.',
    roughness: 'How strong the small swirls are against the big ones.',
    variety: 'How much the small swirls come and go across the picture.',
    field: 'The flow, for Streamlines.',
  },
  push: {
    along: 'A picture whose outlines the flow runs along: the flame, a Shape, the cover.',
    follow: 'How strongly the flow runs along Along’s outlines; below 0 the other way round.',
    reach: 'How far from Along’s edges the flow is felt (its picture blurred that far).',
    amount: 'How far the picture is pushed along the flow.',
    field: 'The flow: a Field’s; without one, the Fluid’s.',
  },
  streamlines: {
    clusters: 'How much the lines gather in clumps, with gaps between.',
    along: 'A picture whose outlines the flow runs along: the flame, a Shape, the cover.',
    follow: 'How strongly the flow runs along Along’s outlines; below 0 the other way round.',
    reach: 'How far from Along’s edges the flow is felt (its picture blurred that far).',
    seeds: 'Where the lines start: a Grid’s points; without one, an even scatter.',
    field: 'The flow they follow: a Field’s; without one, the Fluid’s.',
    length: 'How long each line is.',
    thickness: 'How thick the lines are.',
    calm: 'How still the flow may be before a line fades out.',
    brightness: 'How bright the lines are.',
    hue: 'Their colour, as far as Saturation lets it show.',
    hueSpread: 'How far the colour changes from line to line.',
    saturation: 'At 0 white lines; at 1 fully coloured.',
    perSeed: 'How many lines start round each seed.',
    ring: 'How far round each seed they start.',
    picture: 'The lines.',
  },
  grid: {
    lattice: 'Square, hexagonal, triangular, rings or spiral.',
    count: 'How many points.',
    spacing: 'How far apart the points are.',
    rotation: 'Turns the whole grid.',
    jitter: 'Moves each point off its place at random.',
    drift: 'How fast a flow cabled in carries the points along.',
    return: 'How strongly each point is pulled back to its place.',
    fit: 'Spacing: the points as far apart as Spacing says. Screen: spread out from the count to cover the whole screen.',
    cover: 'With Fit Screen: how far past the screen’s edges the points reach (1: just to them), room to turn and sway.',
    field: 'A flow (a Field’s) that carries the points along.',
    at: 'Points (a Flame’s parts) to lay the grid round: each gets a share of it.',
    points: 'The points, for Fold or Instance.',
    index: 'Each point’s number, from 0 to 1.',
    distance: 'Each point’s distance from the centre.',
    angle: 'Each point’s angle round the centre.',
  },
  lfo: {
    clock: 'Plugged in, Rate counts in beats instead of seconds.',
    reset: 'Starts the wave again; on the Clock’s bar unless unplugged.',
    rate: 'Cycles a second, or a beat while a clock is plugged in.',
    ratio: 'With a clock plugged in, the clock divided or multiplied (÷4 to ×4).',
    shape: 'Sine (0), triangle (1), saw (2), square (3), and blends between.',
    offset: 'Where in its cycle it starts.',
    amount: AMOUNT_OUT,
  },
  random: { rate: 'New random values a second.', shape: 'From smooth glides (0) to stepped jumps (1).', amount: AMOUNT_OUT },
  hold: { rate: 'Samples a second.', glide: 'How long it glides to each new value (seconds); a jump at 0.', full: FULL, amount: AMOUNT_OUT },
  volume: { ...numbered(6, (i) => `band${i}`, () => 'This band’s weight in the sum; left out at 0.'), full: FULL, amount: AMOUNT_OUT },
  fast: { attack: ATTACK, release: RELEASE, full: FULL, amount: AMOUNT_OUT },
  average: { window: 'About how many seconds it averages over.', full: FULL, amount: AMOUNT_OUT },
  peak: { fall: 'How quickly the held peak falls back (seconds).', full: FULL, amount: AMOUNT_OUT },
  mean: { horizon: 'About how many seconds back it remembers.', full: FULL, amount: AMOUNT_OUT },
  gate: {
    above: 'The threshold it opens above (1: above typical).',
    hysteresis: 'How far under the threshold it must fall to close, as a share.',
    lift: 'How much each opening raises the next one’s threshold.',
    recover: 'How quickly that raised threshold falls back (seconds).',
    release: 'How quickly its output falls once it closes (seconds).',
    gap: 'The least time between two openings (seconds; 0: none).',
    amount: AMOUNT_OUT,
  },
  counter: { length: 'Steps to a full count: at 4, beats into it count a bar.', amount: AMOUNT_OUT },
  music: {
    audio: 'The sound itself, its recent samples.',
    playing: '1 while the music plays, 0 while it is paused.',
    cover: 'The playing track’s cover, as a picture: into a Tint for its colours.',
    newtrack: 'A trigger as a track starts, and as the visuals open on one.',
    seed: 'A number of the playing track’s own, the same each time it plays.',
    hits: 'A trigger on each hit in a band, such as a drum or a plucked note, its strength with it.',
    percussiveHits: 'A trigger on each drum hit in a band, held notes left out, its strength with it.',
    hold: 'How long each band’s envelope takes to fall (seconds).',
    ...Object.fromEntries([...TRACK_SIGNALS, ...BAND_SIGNALS].map((s) => [`reference.${s}`, 'How this row is read: against this track’s typical, lately’s, or as it is.'])),
  },
  filterbank: {
    audio: 'The sound to split: Music’s unless unplugged.',
    ...numbered(5, (i) => `split${i}`, (i) => `Where band ${i} ends and band ${i + 1} begins (Hz).`),
    bands: 'The six bands’ sound, one a channel.',
    level: 'Each band’s level, one a channel.',
  },
  meter: {
    motion: 'How much the picture moves.',
    shape: 'How quickly its shapes change.',
    colour: 'How quickly its colours change.',
    zoom: 'How much its size pumps.',
    looks: 'How many new looks a minute.',
  },
  screen: { guard: 'How far the flash guard is holding back now (at most three flashes a second over a large area).' },
  frame: {
    effects: 'The most of every effect’s Amount: the most it reaches; cables into its gain take it lower.',
    loose: 'Set by a double tap of the dice (going with the dice’s Randomize).',
  },
};

/** Ports whose label says all there is (no line): on any type, and on these. */
const PLAIN_ANYWHERE: ReadonlySet<string> = new Set(['picture', 'in', 'out']);
export const PLAIN: Record<string, readonly string[]> = {
  shapes: ['opacity'],
  irregular: ['opacity'],
  waves: ['opacity'],
  clock: ['beat', 'bar', 'phrase'],
  grid: ['row', 'column'],
  mix: ['in2', 'in3', 'in4'],
  sequencer: Array.from({ length: 16 }, (_, i) => `step${i + 1}`),
};

/** The module a setting's knob is on (patch.ts SETTING_KNOB) as its type: the effect slots are their engines. */
const SLOT_TYPE: Record<string, string> = { fx1: 'kaleidoscope-engine', fx2: 'eyedo-engine', fx3: 'hydra-engine' };

/** help.ts's key for a knob or port that has its text there, by type and id. */
export const HELP_KEY: Record<string, Record<string, string>> = (() => {
  const out: Record<string, Record<string, string>> = {};
  const add = (type: string, id: string, key: string) => ((out[type] ??= {})[id] = key);
  // A setting's knob: its setting's text, where it has one.
  for (const [setting, [module, id]] of Object.entries(SETTING_KNOB)) if (HELP[setting]) add(SLOT_TYPE[module] ?? module, id, setting);
  // Show's parts took their knob from Show, id and all.
  for (const part of SHOW_PARTS) if (out.show?.[part.knob]) add(part.type, part.knob, out.show[part.knob]);
  // The flame's per-band times, the walks' cable ends (the old route matrix's), and the knobs the whole frame's
  // settings were.
  for (const id of ['lighting', 'evolution', 'continuity']) add('flame', id, id);
  add('frame', 'lookWalk', 'aspect:look');
  add('frame', 'behaviourWalk', 'aspect:behaviour');
  add('envelope', 'intensity', 'intensity');
  add('clock', 'speed', 'speed');
  // Music's outputs: the signals' texts.
  for (const s of [...TRACK_SIGNALS, ...BAND_SIGNALS]) add('music', s, `signal:${s}`);
  return out;
})();

/** Types the engine has renamed (0057 §7e: Flash is Accent). */
const WAS: Record<string, string> = { flash: 'accent' };

/** Whether a port's label says all (no line wanted). */
export const isPlain = (type: string, id: string) => PLAIN_ANYWHERE.has(id) || (PLAIN[type]?.includes(id) ?? false);

/** The help for a module type, or for one of its ports or knobs (a polyphonic knob's channel, `lighting.3`, as its
 *  knob; a knob's `:gain` or `:next` input, the line all of those share): its own line, else help.ts's text for it;
 *  undefined where the label says it all. A type saved under an old name (`drift`, `flash`) is read as the one it is now. */
export function helpFor(type: string, portOrKnob?: string): string | undefined {
  const t = RENAMED[type] ?? WAS[type] ?? type;
  // A Music signal on its own (`music-<signal>`): Music's line for that output; its reading and hold, Music's.
  const signal = t.startsWith('music-') ? t.slice('music-'.length) : null;
  if (signal) {
    if (portOrKnob === undefined) return `One of the music’s signals on its own: ${(helpFor('music', signal) ?? signal).replace(/^./, (c) => c.toLowerCase())}`;
    if (portOrKnob === 'out') return helpFor('music', signal);
    if (portOrKnob === 'reference') return 'How it is read: against this track’s typical, lately’s, or as it is.';
    return helpFor('music', portOrKnob);
  }
  if (portOrKnob === undefined) return moduleHelp[t];
  const id = portOrKnob.replace(/\.\d+$/, '');
  const suffix = /:(\w+)$/.exec(id)?.[1];
  if (suffix && SUFFIX[suffix]) return SUFFIX[suffix];
  const own = portHelp[t]?.[id];
  if (own) return own;
  const key = HELP_KEY[t]?.[id];
  return key ? HELP[key] : undefined;
}
