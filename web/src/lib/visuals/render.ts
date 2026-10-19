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
import type { ChainNow } from './chain-params';
import { Compositor } from './compose';
import type { FrameInfo, Module, PatchExtra } from './patch';
import { compile, STAGES, stagesOf, type Stage, type TypeOf } from './picture-graph';
import type { PatchV2 } from './ports';
import type { Flame } from './flame';
import type { Flow, Force, Loop } from './fluid';
import { EffectClocks } from './effects';
import type { Tiles } from './meters';
import { Carry } from './nodes/carry';
import { Finish } from './nodes/finish';
import { Light } from './nodes/light';
import { FlamePoints } from './nodes/points';
import { Screen } from './nodes/screen';
import { Show } from './nodes/show';

/** Everything the nodes read in a frame, as one (the frozen renderer's look; chain-params.ts groups it by node). */
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

/** The compose step's inputs this frame (compose.ts: the layers past the flame and the picture's own loop): the
 *  extras as drawn, each module's knobs, the frame, the flame layer's share. */
export type Extras = { extra: PatchExtra; knobsOf: (m: Module) => Record<string, number>; frame: FrameInfo; flame: number };

export class FlameRenderer {
  private gl: WebGL2RenderingContext;
  /** The compose step (compose.ts): the layers past the flame and the picture's own loop, drawn between the show and
   *  the effects when there is anything of them; none where this device cannot. */
  private compositor: Compositor | null = null;
  /** The patch's picture side as drawn (picture-graph.ts): the stages it needs, in this version each fused as before;
   *  every one until a patch is given. Why any of it is not drawn. */
  private stages = new Set<Stage>(STAGES);
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
    try {
      this.compositor = new Compositor(gl);
    } catch (e) {
      console.warn('Visuals: no layers past the flame on this device', e);
    }
  }

  /** The patch to draw (a v2 patch, its picture side wired: v2.ts, picture-wiring.ts), on a change of its structure. */
  setPatch(p: PatchV2, typeOf: TypeOf) {
    const c = compile(p, typeOf);
    const stages = stagesOf(c);
    this.errors = stages ? c.errors : [...c.errors, 'This version draws the picture in its own order only.'];
    this.stages = new Set(stages ?? STAGES);
  }

  /** Lets go of what layers no longer in the extras held. */
  prune(extra: PatchExtra) {
    this.compositor?.prune(extra);
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
  resize(width: number, height: number) {
    if (width === this.light.width && height === this.light.height) return;
    this.carry.resize(width, height);
    this.light.resize(width, height);
  }

  /** The flame to draw from now on (its transforms and colours as uniforms). */
  setFlame(flame: Flame) {
    this.points.setFlame(flame);
  }

  /** One frame: the old picture fades by `now.carry.keep`, `passes` more steps of every point fall into it, and it
   *  shows. */
  frame(passes: number, now: ChainNow, viewWidth: number, viewHeight: number, extras: Extras | null = null) {
    const [screenWidth, screenHeight] = [viewWidth, viewHeight];
    const gl = this.gl;
    const light = this.light;
    if (!this.points.flame || !light.acc || !light.accFbo) return;
    light.tend();
    gl.bindFramebuffer(gl.FRAMEBUFFER, light.accFbo);
    gl.viewport(0, 0, light.width, light.height);
    gl.enable(gl.BLEND);

    const at = this.now();
    const since = this.shownAt ? Math.min(0.25, (at - this.shownAt) / 1000) : 0;
    this.shownAt = at;
    // The loop slot's effect, where there is one.
    const fxl = now.effects;
    this.fxFrames = Math.min(4, since * 60);
    if (fxl) this.clocks.run(fxl);
    const loop =
      fxl && fxl.loop && fxl.loopAmount > 0
        ? { id: fxl.loop, amount: fxl.loopAmount, time: [...this.clocks.times[3], ...this.clocks.moved[3]] as Loop['time'], frames: this.fxFrames, knobs: fxl.loopKnobs }
        : null;
    // Fade what is there; with the Fluid setting, carry it along the flow as it fades, into the other set of buffers
    // (as the scrub copies), and draw on there.
    const fluid = this.carry.draw(light, now.carry, since, loop);
    this.points.draw(light, now.points, passes, since);

    // Show it. The points held at a time: each frame's, kept 1/(1 − keep) frames; their mean per pixel sets k2. Where a
    // chain of effects is set, into the picture at the buffer's size, for the chain's pass to finish.
    const chain = this.stages.has('effects') ? this.fin.chainFor(now.effects, this.clocks) : null;
    const compose = this.stages.has('compose') && extras && this.compositor && Compositor.active(extras.extra, extras.knobsOf, extras.flame) ? this.compositor : null;
    const composing = !!compose;
    const into = chain || composing ? this.fin.targets(light.width, light.height) : null;
    gl.bindFramebuffer(gl.FRAMEBUFFER, into ? into.fbo[2] : null);
    if (into) viewWidth = light.width, viewHeight = light.height;
    this.show.draw(light, this.points, now.show, passes, fluid, this.carry.warpShown, viewWidth, viewHeight);
    const picture = into && compose ? compose.run(into.tex[2], into.w, into.h, extras!.extra, extras!.knobsOf, extras!.frame, extras!.flame) : into ? into.tex[2] : null;
    if (chain && into && picture) this.screen.toScreen(this.fin.finish(chain, now.effects!, into, picture, this.clocks, this.fxFrames, this.carry.fluid), screenWidth, screenHeight);
    else if (into && picture) this.screen.toScreen(picture, screenWidth, screenHeight);
    else this.fin.idle();
  }

  /** The screen just shown (`viewWidth` x `viewHeight`) read into tiles for the flash meter (meters.ts), as of `t`
   *  seconds; read back two or more frames later with `tiles`. */
  measure(viewWidth: number, viewHeight: number, t: number) {
    this.screen.measure(viewWidth, viewHeight, t);
  }

  /** The screen's readings the GPU has finished, the oldest first. Once a frame, before `frame` or anything else draws
   *  (flashmeter.ts). */
  tiles(): Tiles[] {
    return this.screen.tiles();
  }

  /** The flash meter's readings let go unread, its ring full. */
  get meterDropped(): number {
    return this.screen.meterDropped;
  }

  /** Whether the GPU took the context away (the page should stop and drop the renderer). */
  get lost(): boolean {
    return this.gl.isContextLost();
  }

  dispose() {
    this.compositor?.dispose();
    for (const n of [this.carry, this.screen, this.points, this.light, this.show, this.fin]) n.dispose();
    this.gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}
