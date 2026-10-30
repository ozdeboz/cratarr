// Drawing fractal flames on the GPU (decision 0046; the flames themselves are lib/visuals/flame.ts), in WebGL2:
// - points run the chaos game in a vertex shader, one step per pass, their state (position, colour: a place in the
//   colour plane, random seed) carried from pass to pass by transform feedback;
// - each pass also draws them, one pixel each, adding their colour into a half-float buffer that fades a
//   little every frame: the picture is the last second or so of points, which gives the many samples a flame needs
//   without drawing them all every frame, and a little motion blur;
// - the screen shows that buffer by the paper's log-density display: brightness from the log of how many points fell
//   on a pixel, colour from their average, then gamma and vibrancy (its section 4), with a little blur where few fell;
// - beside the colours, a second buffer counts how many of a pixel's points are of each of the music's bands' colours
//   (decision 0046, amended), so a band's transient lights its own colour where it lies;
// - every two seconds both buffers are copied over through a pass that clears anything not a number or infinite: a
//   fade only multiplies, so one such value, once in, would stay a black speck for good.
import type { FieldShape } from './field';
import type { ShowParams, StageValues } from './stage-values';
import type { FrameInfo, Module, PatchExtra } from './patch';
import { compile, composeOf, plan, STAGES, stageOf, stageRefusal, stagesOf, type Compiled, type PictureNode, type Stage, type TypeOf } from './picture-graph';
import { fxOfModule, type Fx } from './nodes/effect-runs';
import { MIX_MODES, SHOW_PARTS, type PatchV2 } from './ports';
import { flatten } from './v2';
import { Walker } from './nodes/walker';
import type { Flame } from './flame';
import type { Fluid, Flow, Force, Loop } from './fluid';
import { EffectClocks } from './effects';
import type { FrameStats, MeterValues, Tiles } from './meters';
import { Carry } from './nodes/carry';
import { Finish } from './nodes/finish';
import { Light } from './nodes/light';
import { Meter } from './nodes/meter';
import { FlamePoints } from './nodes/points';
import { Screen } from './nodes/screen';
import { Thumbs } from './nodes/thumbs';
import { chain, IDENTITY, toScreen, transformOf } from './transform';
import { densityScale, Show } from './nodes/show';
import type { NodeParams } from './engine/graph';
import { stageIdsOf, stagesNow, type StageIds } from './stage-values';
import { none } from './nodes/shared';

/** Show's look with only `parts` of Show's drawn in its pass (by type), the rest at their neutral: no bend, no
 *  smoothing, Haze at 0 (its gamma 3: engine/modules.ts hazeOf), all of its colour. A part elsewhere (Blur after the layers)
 *  is a module of its own. */
function showWith(look: ShowParams, parts: ReadonlySet<string>): ShowParams {
  return {
    ...look,
    ...(parts.has('bend') ? {} : { flow: { ...look.flow, wave: undefined } }),
    ...(parts.has('blur') ? {} : { smoothing: 0 }),
    ...(parts.has('curve') ? {} : { gamma: 3, floor: 0 }),
    ...(parts.has('saturate') ? {} : { saturation: 1 }),
  };
}

/** The modules whose values the light loop and the camera take (ownLoop). */

/** Of `parts`, those as far as `type` in Show's order (a fusion barrier at the observed part). */
const partsUpTo = (parts: ReadonlySet<string>, type: string) => new Set([...parts].filter((t) => SHOW_PARTS.findIndex((p) => p.type === t) <= SHOW_PARTS.findIndex((p) => p.type === type)));

/** The size the cover is drawn at as a picture (16:9, the middle of a square cover). */
const COVER: [number, number] = [320, 180];

/** Everything the nodes read in a frame, as one (stage-values.ts groups it by stage). */
export type Shown = {
  /** Brightness, around 1. */
  brightness: number;
  /** How much of the picture stays from one frame to the next (0..1). */
  keep: number;
  gamma: number;
  vibrancy: number;
  /** The whole picture's turn, radians. */
  turn: number;
  /** How large the flame is drawn: 1 as framed (smaller with more dark around it). */
  scale: number;
  /** How much larger it shows about its middle (a beat's swell): 1 for none, never less. */
  zoom: number;
  /** Brighter by this much as shown (a beat's flash): 1 for none. */
  flash: number;
  /** Each part's light now, low to high (-1..3): its colour's points light up, or below 0 dim. */
  hits: number[];
  /** The most brightness shown (0..1), applied after everything else. */
  ceiling: number;
  /** The display's exposure against the points' mean density (1: as the paper has it): the visuals' auto-exposure. */
  exposure: number;
  /** How much of its colour it shows (0: grey, 1: all). */
  saturation: number;
  /** The most a pixel where few points fell is spread over, in pixels of the buffer (0: none, the raw grain). */
  smoothing: number;
  /** The faintest density shown, on the display's scale (0: all of it; 1: about the flame's mean). */
  floor: number;
  /** How much of a fire's heat shows (0..1): the densest points yellow-white, the faintest a deep red. */
  fire: number;
  /** How much the light flows (0..1, decision 0047): 0 fades it where it was drawn, as before; above, it is carried
   *  along by the fluid (fluid.ts), pushed by `forces` and moving as `flow` says, `dt` seconds a frame, and the picture
   *  is read back along the flow by `warp` seconds of it. */
  fluid: number;
  /** The effects (effects.ts): the picture's chain (its ids and amounts by place, each times the Effects level), the
   *  loop's effect and amount, and the track's own time they move with. None: as before. */
  effects?: {
    ids: number[];
    amounts: number[];
    knobs?: readonly (readonly [number, number])[];
    loop: number;
    loopAmount: number;
    loopKnobs?: readonly [number, number];
    t: number;
    beat?: [number, number, number, number];
  };
  /** Facets (where the light flows: fluid.ts): how much (0..1), and on hexagons rather than squares. */
  facets?: { amount: number; hex: boolean };
  /** The infinite zooms (zoom.ts): how fast, each its log a second (0: off). */
  tunnel: number;
  dive: number;
  forces: Force[];
  flow: Flow;
  dt: number;
  warp: number;
};

export type RendererOptions = { random?: () => number; now?: () => number };

/** What the picture past the flame takes each frame, beside the patch (setPatch): each module's knobs as the engine and
 *  the morph give them now (`knobs`, by the module's id; none: the patch's own), where a knob's morph is going
 *  (`target`; none: where it is), and the frame (the sources' time, beat, signals and sound). */
export type PictureInputs = {
  knobs: (id: string) => Record<string, number> | undefined;
  target?: (id: string, knob: string) => number;
  frame: FrameInfo;
  /** The points a module gives this frame (a Grid's: the engine's `points`, by its id), for Instance and Fold. */
  points?: (moduleId: string) => readonly { x: number; y: number }[] | undefined;
  /** The playing track's cover, loaded (the Music module's picture; none: none yet, or no cover). */
  cover?: () => TexImageSource | null;
  /** Each Field's flow now (the engine's `fields`), by module. */
  fields?: (id: string) => { scale: number; phase: number; waves?: readonly number[]; shape?: FieldShape } | undefined;
  /** What each chain module's node draws with this frame, from its own knobs (the engine's `params`, step 5). */
  params?: (id: string) => NodeParams | undefined;
};

/** The page's extras of today (a v1 PatchExtra): its layers past the flame and the picture's own loop as drawn, each
 *  module's knobs, the frame, the flame layer's share and where its morph is going. */
export type Extras = { extra: PatchExtra; knobsOf: (m: Module) => Record<string, number>; frame: FrameInfo; flame: number; flameTo?: number };

/** The page's extras as picture inputs: the layers' Mix from their blends after the flame's share, each extra's
 *  knobs as moved. */
export function inputsOfExtras(x: Extras): PictureInputs {
  const modules = new Map([...x.extra.layers.flatMap((l) => [l.source, ...l.modules]), ...x.extra.loop].map((m) => [m.id, m]));
  const knobs = (id: string): Record<string, number> | undefined => {
    if (id === 'layers') {
      const k: Record<string, number> = { amount1: x.flame };
      x.extra.layers.forEach((l, i) => {
        k[`mode${i + 2}`] = MIX_MODES.indexOf(l.blend.mode);
        k[`amount${i + 2}`] = l.blend.amount;
      });
      return k;
    }
    const m = modules.get(id);
    return m && x.knobsOf(m);
  };
  return { knobs, target: (id, knob) => (id === 'layers' && knob === 'amount1' ? (x.flameTo ?? 1) : (knobs(id)?.[knob] ?? 0)), frame: x.frame };
}

export class FlameRenderer {
  private gl: WebGL2RenderingContext;
  /** The patch's picture side as drawn (picture-graph.ts): the stages it needs, in this version each fused as before;
   *  every one until a patch is given. Why any of it is not drawn. */
  private stages = new Set<Stage>(STAGES);
  private compiled: Compiled | null = null;
  /** The compose step, the graph's own walk (nodes/walker.ts): its modules, the module whose picture it starts from
   *  (Show's), the one it gives on; each module's knobs as the patch has them. */
  private walker: Walker | null = null;
  private composed: { ids: Set<string>; from: string | null; to: string } | null = null;
  private knobsIn = new Map<string, Record<string, number>>();
  /** Whether the lazy rules cut the flame last frame; whether the last frame was blank. */
  private cut = false;
  private blanked = false;
  /** Each Instance's and Fold's points by the module that gives them. */
  private pointsFrom = new Map<string, string>();
  private fieldFrom = new Map<string, string>();
  /** The Field stirring the Fluid, and how hard (its cable's amount), if one is cabled into it. */
  private fluidStir: { field: string; amount: number } | null = null;
  /** Whether a Grid drifts on the Fluid's flow (its Flow field cabled from the Fluid), and the flow last read back for
   *  it (every few frames: a read waits for the GPU). */
  private gridOnFluid = false;
  private fluidRead: { w: number; h: number; data: Float32Array } | null = null;
  private fluidReads = 0;
  /** The Transforms on the flame's light (the points' camera's view), in order. */
  private lightTransforms: string[] = [];
  /** The module observed (the graph's preview; TouchDesigner's "cook what's observed"): its picture kept for one small
   *  read-back each time a thumbnail asks for it (when last, whether one is wanted), or why it has none this frame.
   *  Nothing observed costs nothing. The last of Show's parts: the one whose picture Show's pass gives. */
  private observed: { id: string; asked: number; wanted: boolean; why: string | null } | null = null;
  private thumbs: Thumbs | null = null;
  private showOut: string | null = null;
  /** The types of Show's parts drawn in its pass (the rest neutral there). */
  private showParts = new Set<string>();
  /** Which module each stage reads (stage-values.ts): Show's parts, the light loop's and the Flame, by type. */
  private stageIds: StageIds = { show: new Map(), loop: new Map() };
  /** A module type's choices (its step knobs), for the compositor's glide between them. */
  private choicesOf: (type: string) => readonly string[] = () => [];
  /** The effect module on the light in its loop (its carry pass draws it), where there is one. */
  private loopEffect: { id: string; type: string } | null = null;
  /** The observed Show part's picture (Show's pass drawn again as far as it), at the buffer's size. */
  private peek: { tex: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number } | null = null;
  /** The Music modules, whether one's cover reaches the Screen, and the cover as a texture (made again as it changes). */
  private musicIds = new Set<string>();
  private coverDrawn = false;
  private cover: { image: TexImageSource; tex: WebGLTexture } | null = null;
  errors: string[] = [];
  /** The context, for what draws beside the flame (the compositor). */
  get context(): WebGL2RenderingContext {
    return this.gl;
  }
  // Its nodes (decision 0057 step 3, nodes/): the light, the points falling into it, its own loop, the show, the
  // effects after it and the screen.
  private light: Light;
  private points: FlamePoints;
  private carry: Carry;
  private show: Show;
  private fin: Finish;
  private screen: Screen;
  private meter: Meter;
  /** The effects' own times (effects.ts), and this frame's length in sixtieths of a second (a stall counted as at most
   *  four, so nothing that feeds back lurches). */
  private clocks = new EffectClocks();
  private fxFrames = 1;
  /** When the last frame was drawn, for the following's clock. */
  private shownAt = 0;
  /** The clock (ms): a replay gives its own (the picture lab), and where the points start. */
  private now: () => number;

  /** The buffer's kind: 32-bit floats where they can be blended into and filtered, else 16-bit (a pixel stops counting
   *  near 2048 points, only in the densest cores, where the log display hardly shows it), else 8 bits (saturating
   *  soon). 16-bit ones were the default in preview 56 (4 to 20 % quicker): the densest pixels reach some 60,000 points
   *  (measured in 32-bit ones, 2026-10-18), against a 16-bit float's 65,504, and a GPU whose blending rounds up
   *  (Direct3D allows it) carries them over into infinity, black for good: a GTX 1650 showed such specks from 56 on, a
   *  tablet's Mali-G52 (16-bit then, a GPU that rounds to nearest, stuck at 2048) never did. 32-bit ones that cannot be
   *  filtered (that tablet's Mali-G52 blends them but has no OES_texture_float_linear, 2026-10-18; a phone's Mali-G615
   *  has it) were read pixel by pixel and scaled up to the screen blocky: the tablet's picture grainier in 59, and no
   *  coarser levels for the smoothing. */
  get precision(): 32 | 16 | 8 {
    return this.light.precision;
  }
  set precision(p: 32 | 16 | 8) {
    this.light.precision = p;
  }
  get halfFloat(): boolean {
    return this.light.precision !== 8;
  }

  /** `points`: how many carry the chaos game at first. `power`: which GPU a computer with two should draw on (the
   *  fast one: by default a browser may take the one that saves power, several times slower). */
  constructor(canvas: HTMLCanvasElement, points: number, power: WebGLPowerPreference = 'high-performance', options: RendererOptions = {}) {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, preserveDrawingBuffer: false, powerPreference: power });
    if (!gl) throw new Error('WebGL2 is not available.');
    this.gl = gl;
    this.now = options.now ?? (() => performance.now());
    this.light = new Light(gl);
    this.points = new FlamePoints(gl, points, options.random ?? Math.random, this.now);
    this.carry = new Carry(gl, this.now);
    this.show = new Show(gl);
    this.fin = new Finish(gl, this.now);
    this.screen = new Screen(gl);
    this.meter = new Meter(gl);
  }

  /** The patch to draw (a v2 patch, its picture side wired: v2.ts, picture-wiring.ts), on a change of its structure. */
  setPatch(p: PatchV2, typeOf: TypeOf) {
    const c = compile(p, typeOf);
    const stages = stagesOf(c);
    this.compiled = stages ? c : null;
    this.errors = stages ? c.errors : [...c.errors, stageRefusal(c) ?? 'Not drawn in its order.'];
    this.stages = new Set(stages ?? STAGES);
    const flat = flatten(p);
    this.knobsIn = new Map(flat.modules.map((m) => [m.id, m.knobs]));
    // What gives each Instance its points and each Fold its grid.
    const typeOfId = new Map(flat.modules.map((m) => [m.id, m.type]));
    this.pointsFrom = new Map(flat.cables.filter((c) => (c.to.port === 'points' && typeOfId.get(c.to.module) === 'instance') || (c.to.port === 'grid' && typeOfId.get(c.to.module) === 'fold') || (c.to.port === 'seeds' && typeOfId.get(c.to.module) === 'streamlines')).map((c) => [c.to.module, c.from.module]));
    // What gives each Streamlines its flow: a Field cabled into it (else the Fluid's, the normal).
    this.fieldFrom = new Map(flat.cables.filter((c) => c.to.port === 'field' && ['streamlines', 'push', 'instance'].includes(typeOfId.get(c.to.module) ?? '') && typeOfId.get(c.from.module) === 'field').map((c) => [c.to.module, c.from.module]));
    const stir = flat.cables.find((c) => c.to.port === 'stirBy' && typeOfId.get(c.to.module) === 'fluid' && typeOfId.get(c.from.module) === 'field');
    this.fluidStir = stir ? { field: stir.from.module, amount: typeof stir.value === 'number' ? stir.value : 1 } : null;
    this.gridOnFluid = flat.cables.some((c) => c.to.port === 'field' && typeOfId.get(c.to.module) === 'grid' && typeOfId.get(c.from.module) === 'fluid');
    if (!this.gridOnFluid) this.fluidRead = null;
    this.composed = stages ? composeOf(c) : null;
    this.lightTransforms = stages ? c.order.filter((n) => n.type === 'transform' && stageOf(n) === 'points').map((n) => n.id) : [];
    this.showOut = stages ? (c.order.filter((n) => stageOf(n) === 'show').at(-1)?.id ?? null) : null;
    this.showParts = new Set(stages ? c.order.filter((n) => stageOf(n) === 'show').map((n) => n.type) : SHOW_PARTS.map((p) => p.type));
    const onLight = stages ? c.order.find((n) => stageOf(n) === 'carry' && fxOfModule(n.type, {})) : undefined;
    this.loopEffect = onLight ? { id: onLight.id, type: onLight.type } : null;
    this.stageIds = stageIdsOf(p, typeOf);
    // Each module type's choices (its step knobs): the compositor glides from one to the next.
    const choices = new Map<string, readonly string[]>();
    this.choicesOf = (type) => {
      if (!choices.has(type)) choices.set(type, (typeOf(type)?.knobs ?? []).filter((k) => k.kind === 'step').map((k) => k.id));
      return choices.get(type)!;
    };
    this.musicIds = new Set(flat.modules.filter((m) => m.type === 'music').map((m) => m.id));
    this.coverDrawn = !!stages && c.order.some((n) => n.type === 'music');
    this.walker?.prune(this.composed?.ids ?? new Set());
  }

  /** Lets go of what modules no longer in the patch held. setPatch does it too; the page of today calls this as its
   *  extras change (`_extra`: for the lab's recordings). */
  prune(_extra?: PatchExtra) {
    this.walker?.prune(this.composed?.ids ?? new Set());
  }

  /** Whether the playing track's cover is wanted as a picture (a Music module's reaches the Screen, or is observed):
   *  the page loads it then (`PictureInputs.cover`). */
  get wantsCover(): boolean {
    return this.coverDrawn || (!!this.observed && this.musicIds.has(this.observed.id));
  }

  get width(): number {
    return this.light.width;
  }

  get height(): number {
    return this.light.height;
  }

  /** How many points carry the chaos game: fewer where the device cannot keep up (they start again at random). */
  setPoints(n: number) {
    this.points.setPoints(n);
  }

  get pointCount(): number {
    return this.points.pointCount;
  }

  /** Whether points added lately are still settling, or a program was just made (an effect's, as it is first chosen:
   *  making it makes a frame or two late, some while after): the frames meanwhile are not the device's usual pace. */
  get settling(): boolean {
    return this.points.settling || this.now() - Math.max(this.carry.madeAt, this.fin.madeAt) < 600;
  }

  /** How fast the dive turns the picture now (radians a second): the motion meter counts it with the parts' own. */
  get diveTurnRate(): number {
    return this.points.diveTurnRate;
  }

  /** How many scrubs found anything not a number or infinite in the buffers (the black specks' cause, if they come). */
  get scrubbed(): number {
    return this.light.scrubbed;
  }

  /** The drawing size in pixels (the buffer the points fall into; the screen scales it). */
  /** The Fluid's flow read back for a Grid drifting on it, every third frame (the framebuffer bound before kept). */
  private readFluid(fluid: Fluid | null) {
    if (!fluid) {
      this.fluidRead = null;
      return;
    }
    if (this.fluidReads++ % 3 && this.fluidRead) return;
    const gl = this.gl;
    const n = fluid.width * fluid.height * 4;
    const r = this.fluidRead && this.fluidRead.data.length === n ? this.fluidRead : { w: fluid.width, h: fluid.height, data: new Float32Array(n) };
    const bound = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    if (fluid.read(r.data)) this.fluidRead = { ...r, w: fluid.width, h: fluid.height };
    gl.bindFramebuffer(gl.FRAMEBUFFER, bound);
  }

  /** The Fluid's flow where a Grid reads it (the frame's units: centred, the short side −1..1; a second), from the last
   *  read; undefined without one. */
  fluidFlow(): ((x: number, y: number) => [number, number]) | undefined {
    const r = this.fluidRead;
    if (!r) return undefined;
    const [kx, ky] = toScreen(this.width / Math.max(1, this.height));
    return (x, y) => {
      const cx = Math.min(r.w - 1, Math.max(0, Math.floor((x * kx * 0.5 + 0.5) * r.w)));
      const cy = Math.min(r.h - 1, Math.max(0, Math.floor((y * ky * 0.5 + 0.5) * r.h)));
      const i = (cy * r.w + cx) * 4;
      // Cells a second, to texture coordinates a second, to the frame's units a second.
      return [((r.data[i] / r.w) * 2) / kx, ((r.data[i + 1] / r.h) * 2) / ky];
    };
  }

  resize(width: number, height: number) {
    if (width === this.light.width && height === this.light.height) return;
    this.carry.resize(width, height);
    this.light.resize(width, height);
  }

  /** The flame to draw from now on (its transforms and colours as uniforms). */
  setFlame(flame: Flame) {
    this.points.setFlame(flame);
  }

  /** One frame of the patch: the flame (its light faded by `now.carry.keep`, `passes` more steps of every point in it,
   *  shown), what the compose step draws, the effects, onto the screen. `now` null: no flame data (no Cratarr chain):
   *  its stages are skipped and the rest drawn as the patch wires it; where nothing reaches the screen, it is blank. */
  frame(passes: number, now: StageValues | null, viewWidth: number, viewHeight: number, inputs: PictureInputs | Extras | null = null) {
    const given = inputs && 'extra' in inputs ? inputsOfExtras(inputs) : inputs;
    const knobsAt = (id: string) => given?.knobs(id) ?? this.knobsIn.get(id) ?? {};
    // Each stage with its own modules' values (stage-values.ts), the device's where it has none.
    if (now) now = stagesNow(now, this.stageIds, (id) => given?.params?.(id));
    const shown = now ? now.show : null;
    const [screenWidth, screenHeight] = [viewWidth, viewHeight];
    const gl = this.gl;
    const light = this.light;
    const flamed = !!now && !!this.points.flame && !!light.acc && !!light.accFbo;
    if (now && !flamed) return;
    const seeing = this.seeing();
    // Where in the light's loop an observed light module's picture is (none: not a light module).
    const seenNode = seeing ? this.compiled?.order.find((x) => x.id === seeing) : undefined;
    const lightPhase = !seenNode ? null : seenNode.type === 'feedback' && seenNode.inputs[0]?.type === 'light' ? 'held' : stageOf(seenNode) === 'carry' ? 'carried' : stageOf(seenNode) === 'points' ? 'lit' : null;
    let lightPeek: WebGLTexture | null = null;
    if (seeing) this.thumbs?.poll();
    const at = this.now();
    const since = this.shownAt ? Math.min(0.25, (at - this.shownAt) / 1000) : 0;
    this.shownAt = at;
    // The light loop's effect: the effect module on the light in its loop, as its knobs are (none without one; the
    // chain's loop slot where the patch is not compiled, the labs' recorded looks).
    const onLight = this.loopEffect ? fxOfModule(this.loopEffect.type, knobsAt(this.loopEffect.id)) : null;
    const chainFx = now?.effects;
    const fxl = chainFx && this.compiled ? { ...chainFx, ids: [0, 0, 0], amounts: [0, 0, 0], loop: onLight?.id ?? 0, loopAmount: onLight?.amount ?? 0, loopKnobs: onLight?.k ?? ([0.5, 0.5] as [number, number]) } : chainFx;
    this.fxFrames = Math.min(4, since * 60);
    if (fxl) this.clocks.run(fxl);
    const loop =
      fxl && fxl.loop && fxl.loopAmount > 0
        ? { id: fxl.loop, amount: fxl.loopAmount, time: [...this.clocks.times[3], ...this.clocks.moved[3]] as Loop['time'], frames: this.fxFrames, knobs: fxl.loopKnobs }
        : null;
    // The lazy rules (picture-graph.ts plan): the flame's layer at nothing, and not coming back, cuts the flame and all
    // that feeds only it (decision 0057, its 5th: one coming back keeps gathering its light). The rest is lazy within
    // its pass: an effect, a layer, the loop at nothing.
    const drawn = flamed && (!this.compiled || !given || plan(this.compiled, { knobs: knobsAt, target: given.target }).live.some((n) => n.type === 'flame'));
    let fluid: Fluid | null = null;
    if (drawn && this.cut) light.clear();
    this.cut = !drawn;
    if (drawn) {
      light.tend();
      gl.bindFramebuffer(gl.FRAMEBUFFER, light.accFbo);
      gl.viewport(0, 0, light.width, light.height);
      gl.enable(gl.BLEND);
      // Fade what is there; with the Fluid setting, carry it along the flow as it fades, into the other set of
      // buffers (as the scrub copies), and draw on there.
      // An observed light module's picture (step 6): the light tone mapped where it is in the loop, a carry's as it
      // carries it on, the Flame's (and a Transform on its light) as its points land; the Feedback's what it holds for
      // the next frame, the light as the points land (tone mapping it before the carries reads it unlike Show does).
      const stirring = this.fluidStir ? given?.fields?.(this.fluidStir.field) : undefined;
      const carry = stirring ? { ...now!.carry, flow: { ...now!.carry.flow, field: { ...stirring, amount: this.fluidStir!.amount } } } : now!.carry;
      // Lift reads the light on the display's scale, as Show will (the points as the last frame left them).
      fluid = this.carry.draw(light, carry, since, loop, densityScale(light, this.points.ready, passes, shown!.keep, shown!.exposure));
      if (this.gridOnFluid) this.readFluid(fluid);
      if (lightPhase === 'carried') lightPeek = this.peekLight(shown!, passes, fluid);
      // The light's Transforms, in the patch's order: where the points land, after the camera frames the flame.
      this.points.view = this.lightTransforms.reduce((t, id) => chain(t, transformOf(knobsAt(id), light.width / light.height)), IDENTITY);
      this.points.draw(light, now!.points, passes, since);
      // After the points, the light is drawn: blending off again as they left it (left on, Show's pass added the picture
      // onto the last frame's until it was white: the owner's preview bug, 73).
      if (lightPhase === 'lit' || lightPhase === 'held') {
        lightPeek = this.peekLight(shown!, passes, fluid);
        gl.disable(gl.BLEND);
      }
    }

    // Show it. The points held at a time: each frame's, kept 1/(1 − keep) frames; their mean per pixel sets k2. Where the
    // compose step walks on from it, into the picture at the buffer's size.
    // Each effect as its module's knobs are (the engine's: its own columns, step 5).
    const fx = now?.effects;
    const fxOf = (n: PictureNode): Fx | null => fxOfModule(n.type, given?.knobs(n.id) ?? this.knobsIn.get(n.id) ?? {});
    const walks = this.stages.has('compose') && this.compiled && this.composed;
    if (walks) {
      this.walker ??= new Walker(gl, this.fin, this.now);
      this.walker.steps = this.choicesOf;
    }
    const walking = walks ? this.walker!.planned(this.compiled!, this.composed!.ids, this.composed!.from, this.composed!.to, knobsAt, fxOf, seeing) : null;
    const cover = this.wantsCover ? this.coverOf(given?.cover?.() ?? null) : null;
    if (!drawn && !walking) {
      if (seeing) this.observed!.why = 'Nothing reaches the Screen.';
      return this.blank(screenWidth, screenHeight);
    }
    this.blanked = false;
    // Observed, Show's picture is drawn into a target even where nothing walks on from it (then copied to the screen).
    const source = seeing && walks ? this.walker!.source(seeing) : seeing;
    const into = walking || (drawn && !!source && source === this.showOut) ? this.fin.targets(light.width, light.height) : null;
    if (drawn) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, into ? into.fbo : null);
      if (into) viewWidth = light.width, viewHeight = light.height;
      this.show.draw(light, this.points, showWith(shown!, this.showParts), passes, fluid, this.showParts.has('bend') ? this.carry.warpShown : 0, viewWidth, viewHeight);
    }
    const info = given?.frame ?? { dt: since, time: at / 1000, beat: null, signals: () => 0, waveform: null };
    // Without the flame drawn, the compose step starts from nothing where Show's picture would be.
    const effects = { t: fx?.t ?? info.time, beat: fx?.beat, frames: this.fxFrames, fluid: this.carry.fluid };
    const pointsOf = (id: string) => (this.pointsFrom.has(id) ? (given?.points?.(this.pointsFrom.get(id)!) ?? null) : null);
    const covers = new Map([...this.musicIds].map((id) => [id, cover ?? none(gl)]));
    const fieldsOf = (id: string) => (this.fieldFrom.has(id) ? (given?.fields?.(this.fieldFrom.get(id)!) ?? null) : null);
    const picture = into && walking ? this.walker!.run(walking, drawn ? into.tex : null, into.w, into.h, knobsAt, info, effects, pointsOf, seeing, covers, fieldsOf) : null;
    if (picture) this.screen.toScreen(picture, screenWidth, screenHeight);
    else if (into) this.screen.toScreen(into.tex, screenWidth, screenHeight);
    else this.fin.idle();
    if (seeing) {
      const n = this.compiled?.order.find((x) => x.id === seeing);
      const parts = n ? partsUpTo(this.showParts, n.type) : null;
      const part = drawn && n && parts && stageOf(n) === 'show' && n.id !== this.showOut ? this.peekShow(showWith(shown!, parts), passes, fluid, parts.has('bend')) : null;
      this.look(seeing, this.seenOf(seeing, source, walking ? this.walker!.seen : null, drawn ? into : null, part, cover, lightPeek));
      this.walker?.unpin();
    }
  }

  /** Show's pass drawn again into the peek target with `look` (as far as the observed part): its picture. */
  private peekShow(look: ShowParams, passes: number, fluid: Fluid | null, bent: boolean): WebGLTexture {
    const gl = this.gl;
    const [w, h] = [this.light.width, this.light.height];
    if (this.peek && (this.peek.w !== w || this.peek.h !== h)) this.freePeek();
    if (!this.peek) {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      this.peek = { tex, fbo, w, h };
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.peek.fbo);
    this.show.draw(this.light, this.points, look, passes, fluid, bent ? this.carry.warpShown : 0, w, h);
    return this.peek.tex;
  }

  /** The light as it is now, tone mapped alone (Show's other parts at their neutral), into the peek target; the light's
   *  own drawing state given back after (its buffers bound, blending on), as the loop goes on drawing into it. */
  private peekLight(look: ShowParams, passes: number, fluid: Fluid | null): WebGLTexture {
    const gl = this.gl;
    const tex = this.peekShow(showWith(look, new Set(['tonemap'])), passes, fluid, false);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.light.accFbo);
    gl.viewport(0, 0, this.light.width, this.light.height);
    gl.enable(gl.BLEND);
    return tex;
  }

  private freePeek() {
    if (!this.peek) return;
    this.gl.deleteTexture(this.peek.tex);
    this.gl.deleteFramebuffer(this.peek.fbo);
    this.peek = null;
  }

  /** The cover as a texture, the middle of it at 16:9 (made once a cover); null for none. */
  private coverOf(image: TexImageSource | null): WebGLTexture | null {
    if (!image) return null;
    if (this.cover?.image === image) return this.cover.tex;
    const gl = this.gl;
    const c = document.createElement('canvas');
    [c.width, c.height] = COVER;
    const g = c.getContext('2d');
    if (!g) return null;
    const { width: iw, height: ih } = 'naturalWidth' in image ? { width: image.naturalWidth, height: image.naturalHeight } : (image as { width: number; height: number });
    const r = Math.min(iw / COVER[0], ih / COVER[1]);
    g.drawImage(image as CanvasImageSource, (iw - COVER[0] * r) / 2, (ih - COVER[1] * r) / 2, COVER[0] * r, COVER[1] * r, 0, 0, COVER[0], COVER[1]);
    const tex = this.cover?.tex ?? gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, c);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.cover = { image, tex };
    return tex;
  }

  /** The module observed from now on (a flattened id: `show/saturate`; null: none). `thumbnail` observes too. */
  observe(moduleId: string | null) {
    this.observed = moduleId ? { id: moduleId, asked: performance.now(), wanted: true, why: null } : null;
  }

  /** `moduleId`'s picture onto `canvas`, a sixteenth of the buffer's sides, as last read back (two or more frames
   *  late, never stalling); it is observed from now on, for a second after the last ask. Null where it shows (or soon
   *  will); else why it has none: on the light, fused into a pass, reaching no Screen. */
  thumbnail(moduleId: string, canvas: HTMLCanvasElement): string | null {
    const o = this.observed;
    if (o?.id === moduleId) Object.assign(o, { asked: performance.now(), wanted: true });
    else this.observe(moduleId);
    if (this.thumbs?.put(moduleId, canvas)) return null;
    return this.observed!.why;
  }

  /** The module observed now (none a second after the last ask). */
  private seeing(): string | null {
    if (this.observed && performance.now() - this.observed.asked > 1000) this.observed = null;
    return this.observed?.id ?? null;
  }

  /** What observed module `id` shows this frame (`source`: the module whose picture it passes on; `walked`: what the
   *  walk says of it; `shown`: Show's target, where Show drew into one; `part`: Show drawn as far as an observed part;
   *  `cover`: the Music's). */
  private seenOf(id: string, source: string | null, walked: { tex: WebGLTexture } | { why: string } | null, shown: { tex: WebGLTexture } | null, part: WebGLTexture | null, cover: WebGLTexture | null, lit: WebGLTexture | null = null): { tex: WebGLTexture; size?: [number, number] } | { why: string } {
    if (this.musicIds.has(id)) return cover ? { tex: cover, size: COVER } : { why: 'No cover.' };
    if (!this.compiled) return { why: this.errors.at(-1) ?? 'Not drawn.' };
    const n = this.compiled.order.find((x) => x.id === id);
    if (!n) return { why: 'Its picture reaches no Screen.' };
    const stage = stageOf(n);
    if (stage === 'carry' || stage === 'points' || (n.type === 'feedback' && n.inputs[0]?.type === 'light')) return lit ? { tex: lit } : { why: 'Not drawn this frame.' };
    if (source === this.showOut) return shown ? { tex: shown.tex } : { why: 'Not drawn this frame.' };
    if (stage === 'show') return part ? { tex: part } : { why: 'Not drawn this frame.' };
    return walked ?? { why: 'Not drawn this frame.' };
  }

  /** The observed module's picture read back small where a thumbnail wants one (one reading in flight at a time). */
  private look(id: string, s: { tex: WebGLTexture; size?: [number, number] } | { why: string }) {
    const o = this.observed!;
    if ('why' in s) {
      o.why = s.why;
      this.thumbs?.forget();
      return;
    }
    o.why = null;
    this.thumbs ??= new Thumbs(this.gl);
    if (!o.wanted || !this.thumbs.idle) return;
    const [w, h] = s.size ?? [this.light.width, this.light.height];
    this.thumbs.capture(id, s.tex, w, h);
    o.wanted = false;
  }

  /** The screen black, and what the picture kept from frame to frame let go of: the flame's light, the loops' and the
   *  echoes' last pictures (so a chain coming back starts from nothing). `frame` does it itself where nothing reaches
   *  the screen; the screen is cleared each such frame, the rest once. */
  blank(screenWidth = this.gl.drawingBufferWidth, screenHeight = this.gl.drawingBufferHeight) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, screenWidth, screenHeight);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (this.blanked) return;
    this.blanked = true;
    if (this.light.accFbo) this.light.clear();
    this.cut = true;
    this.walker?.forget();
    this.fin.freeFx();
  }

  /** The Meter on what the screen just showed (`viewWidth` x `viewHeight`, as of `t` seconds: nodes/meter.ts):
   *  read back two or more frames later with `tiles`. */
  measure(viewWidth: number, viewHeight: number, t: number) {
    this.meter.capture(viewWidth, viewHeight, t);
  }

  /** The Meter's readings the GPU has finished, the oldest first. Once a frame, before `frame` or anything else draws
   *  (flashmeter.ts). */
  tiles(): Tiles[] {
    return this.meter.read();
  }

  /** The frame's own numbers into the Meter, with its readings in since; its values (meters.ts) for the engine. */
  meterAdd(stats: Omit<FrameStats, 'tiles'>) {
    this.meter.add(stats);
  }

  get meterValues(): MeterValues {
    return this.meter.values();
  }

  /** The Meter measuring anew (a trial of the rating page). */
  meterReset() {
    this.meter.reset();
  }

  /** The Meter's readings let go unread, its ring full. */
  get meterDropped(): number {
    return this.meter.dropped;
  }

  /** Whether the GPU took the context away (the page should stop and drop the renderer). */
  get lost(): boolean {
    return this.gl.isContextLost();
  }

  dispose() {
    this.walker?.dispose();
    this.thumbs?.dispose();
    this.freePeek();
    if (this.cover) this.gl.deleteTexture(this.cover.tex);
    for (const n of [this.carry, this.screen, this.meter, this.points, this.light, this.show, this.fin]) n.dispose();
    this.gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}

