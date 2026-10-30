// The Flame's own work beside its parts' motion (chain.ts stepFlame), each Flame its own state, stepped by the engine
// (graph.ts) where it runs with a view: what the frame loop worked out for the Flame until the owner's "engine purity"
// (2026-10-29), so a Flame's knobs and outputs are the whole of what draws it.
// - Its look: made from its own knobs (as the engine sets them, their macros in) when they change, in the colours its
//   Colours knob chooses (Cover's once the playing track's are in), blended in from what was drawn over the time its
//   cause gives (a new track's look, the dice's, a step moved on) or, unasked, a second and a half; one moved only by
//   degrees (Detail, Twist, Warp, Hue, Lightness, Vividness) at once, as a knob dragged is.
// - Its parts' turns, Purity and Spread on that look (evolved).
// - Framed as far as its Fit: where its points fall, measured a slice a frame every half second and followed over a
//   second and a half, the camera centred and sized to them at 1, left where they fall at 0.
// - Where its parts are on the screen, four times a second while something reads them (the Fluid's pushes, a Grid's
//   or Streamlines' seeds), and its outputs: Centre across, Centre up, Size, Coverage (a Transform's units).

import { between, coloured, evolved, flameOf, measuring, partsAt, type ColourMode, type Flame, type Preset } from '../flame';
import type { PaletteColour } from '../palette';
import type { FlameNow } from './chain';

/** The knobs a Flame's look is made of (flame.ts build), and those it follows by degrees. */
export const LOOK_KNOBS = ['form', 'detail', 'twist', 'warp', 'symmetry', 'mirror', 'final', 'lensSides', 'lensDepth', 'family', 'hue', 'lightness', 'vividness'] as const;
const BY_DEGREES = new Set<string>(['detail', 'twist', 'warp', 'hue', 'lightness', 'vividness']);
/** A change of look nobody timed blends in over this long. */
export const LOOK_MS = 1500;

/** Its Colours knob's colours: Spectrum to Three are its own hues (0 to 3), then Cover (4), then Fire (5). */
export const colourModeOf = (family: number): ColourMode => (family >= 5 ? 'fire' : family >= 4 ? 'cover' : 'rainbow');

/** Where a Flame is and how much of the view it covers (its outputs). */
export type FlameFrame = { x: number; y: number; size: number; coverage: number };

export type FlameNodeState = {
  made: Record<string, number> | null;
  madeColours: string;
  base: Flame | null;
  from: Flame | null;
  to: Flame | null;
  shown: Flame | null;
  blendStart: number;
  blendMs: number;
  /** The fire's share, followed over a second and a half as Colours goes to Fire and back. */
  fire: number;
  /** Where its points fall, as last measured and followed; the measure under way; when it was begun. */
  seen: { center: [number, number]; size: number; coverage: number } | null;
  measure: ReturnType<typeof measuring> | null;
  measured: number;
  /** Its parts on the screen (0..1, y up), when they were last found, and its outputs. */
  parts: ([number, number] | null)[];
  partsAt: number;
  frame: FlameFrame | null;
};

export const newFlameNode = (): FlameNodeState => ({
  made: null,
  madeColours: '',
  base: null,
  from: null,
  to: null,
  shown: null,
  blendStart: 0,
  blendMs: LOOK_MS,
  fire: 0,
  seen: null,
  measure: null,
  measured: 0,
  parts: [],
  partsAt: 0,
  frame: null,
});

/** What a Flame draws with this frame. */
export type FlameView = {
  /** Its look as it blends, its parts on it (the renderer's flame), and framed (its camera). */
  drawn: Flame;
  camera: Flame;
  /** Its colours: whether it wants the cover's (the page fetches them), and the fire's share. */
  mode: ColourMode;
  fire: number;
  /** Its parts on the screen (0..1, y up), as last found. */
  parts: readonly ([number, number] | null)[];
};

export type FlameInputs = {
  /** Its knobs as the engine sets them (its macros in; a knob it lacks, its type's default), and its Fit as it moves. */
  base: (knob: string) => number;
  fit: number;
  /** Its parts' motion this frame, its size (Zoom), the frame's time and length. */
  now: FlameNow;
  scale: number;
  at: number;
  dt: number;
  /** The playing track's key and its cover's colours, as the page has them (null till they come). */
  cover: { key: string | null; colours: PaletteColour[] | null };
  /** How long this frame's change of look glides (a new track's, the dice's, a step's), null: as a knob's. */
  glide: number | null;
  /** The picture's width over its height, and whether something reads its parts on the screen. */
  aspect: number;
  readsParts: boolean;
};

/** One frame of a Flame's look, framing and place on the screen. */
export function stepFlameNode(st: FlameNodeState, o: FlameInputs): FlameView {
  const look = o.base;
  const mode = colourModeOf(look('family'));
  const covered = mode === 'cover' && o.cover.key && o.cover.colours ? o.cover.colours : null;
  st.fire += ((mode === 'fire' ? 1 : 0) - st.fire) * Math.min(1, o.dt / 1.5);
  const colours = `${mode}/${covered ? o.cover.key : ''}`;
  const changed = !st.made ? [...LOOK_KNOBS] : LOOK_KNOBS.filter((k) => st.made![k] !== look(k));
  if (changed.length || colours !== st.madeColours) {
    if (changed.length) st.base = flameOf(Object.fromEntries(LOOK_KNOBS.map((k) => [k, look(k)])) as unknown as Preset, false);
    // The cover's colours coming in for the same look: they blend over the look's time already running.
    const coverCame = !changed.length && st.madeColours.startsWith('cover/') && colours.startsWith('cover/') && st.from;
    const blend = o.glide ?? (changed.every((k) => BY_DEGREES.has(k)) && colours === st.madeColours ? 0 : coverCame ? null : LOOK_MS);
    // A change while an untimed blend is under way (a macro turning a choice, frame after frame) only moves where it is
    // going: begun anew each frame from what shows, the look stood still until the knob did (0.10; the owner,
    // 2026-10-29: continuous over discrete). A timed one (a new track's, the dice's, a step's) begins anew.
    const blending = st.from !== null && o.at - st.blendStart < st.blendMs;
    if (blend && st.shown && !(blending && o.glide === null)) {
      st.from = st.shown;
      st.blendStart = o.at;
      st.blendMs = blend;
    }
    st.made = Object.fromEntries(LOOK_KNOBS.map((k) => [k, look(k)]));
    st.madeColours = colours;
    st.to = coloured(st.base!, mode, covered);
    if (!st.from) st.shown = st.to;
  }
  const k = st.from ? Math.min(1, (o.at - st.blendStart) / st.blendMs) : 1;
  st.shown = st.from && k < 1 ? between(st.from, st.to!, k * k * (3 - 2 * k)) : st.to!;
  if (k >= 1) st.from = null;
  const drawn = evolved(st.shown, o.now.parts, o.base('purity'), o.base('spread'));

  // Where its points fall: a slice of the measure each frame, a new one every half second (the first all at once).
  if (!st.measure && o.at - st.measured > 500) {
    st.measured = o.at;
    st.measure = st.seen ? measuring(drawn) : measuring(drawn, 6000, 6000);
  }
  const measuredNow = st.measure?.next();
  if (measuredNow?.done) {
    st.measure = null;
    const seen = measuredNow.value;
    if (seen) {
      if (!st.seen) st.seen = seen;
      else {
        const kk = 1 - Math.exp(-0.5 / 1.5);
        st.seen = {
          center: [st.seen.center[0] + (seen.center[0] - st.seen.center[0]) * kk, st.seen.center[1] + (seen.center[1] - st.seen.center[1]) * kk],
          size: Math.exp(Math.log(st.seen.size) + (Math.log(seen.size) - Math.log(st.seen.size)) * kk),
          coverage: st.seen.coverage + (seen.coverage - st.seen.coverage) * kk,
        };
      }
    }
  }
  const seen = st.seen;
  const fitting = Math.min(1, Math.max(0, o.fit));
  const camera =
    seen && fitting > 0
      ? {
          ...drawn,
          center: [drawn.center[0] + (seen.center[0] - drawn.center[0]) * fitting, drawn.center[1] + (seen.center[1] - drawn.center[1]) * fitting] as [number, number],
          size: Math.exp(Math.log(drawn.size) + (Math.log(seen.size) - Math.log(drawn.size)) * fitting),
        }
      : drawn;
  // Its outputs: where it is on the screen and how much it covers, as measured.
  if (seen) {
    const s = o.scale / camera.size;
    st.frame = { x: (seen.center[0] - camera.center[0]) * s, y: (seen.center[1] - camera.center[1]) * s, size: seen.size * s, coverage: seen.coverage };
  }

  // Its parts on the screen, four times a second while something reads them.
  if (o.readsParts && o.at - st.partsAt > 250) {
    st.partsAt = o.at;
    const cam = seen ?? drawn;
    const cos = Math.cos(o.now.turn);
    const sin = Math.sin(o.now.turn);
    st.parts = [];
    for (const p of partsAt(drawn)) {
      if (!p) {
        st.parts.push(null);
        continue;
      }
      let x = ((p[0] - cam.center[0]) * o.scale) / cam.size;
      let y = ((p[1] - cam.center[1]) * o.scale) / cam.size;
      [x, y] = [cos * x - sin * y, sin * x + cos * y];
      if (o.aspect > 1) x /= o.aspect;
      else y *= o.aspect;
      st.parts.push([x * 0.5 + 0.5, y * 0.5 + 0.5]);
    }
  }
  return { drawn, camera, mode, fire: st.fire, parts: st.parts };
}
