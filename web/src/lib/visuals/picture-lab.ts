// The picture lab (decision 0057, step 3; development only, picture-lab.html): the picture-diff harness the picture
// graph's steps are proved by, frame by frame against today's renderer.
//   ?replay=<name>&a=<pipeline>&b=<pipeline>&bits=32|16|8   a recording (tests/pictures/<name>.json.gz) through two
//                              pipelines, the same points and clock for both, each frame's screen read back and
//                              compared: the largest and mean difference (of 255), the PSNR, and the flash meter's tiles;
//                              &seedB=n starts the second one's points elsewhere (the harness's own check: it differs)
// The recordings were made from the page before the switch (recorder.ts); the page on the store records no more.
// window.pictureLab holds the result; web/scripts/check-picture.mjs drives it.
import { ENGINE_EFFECTS, ENGINES } from './effects';
import { randomFrom } from './flame';
import { Meters, type FrameStats, type MeterValues, type Tiles } from './meters';
import type { FrameInfo, Module } from './patch';
import { emptyExtra } from './patch';
import { flameOf, lookOf, signalKey, type RecordedCompose, type RecordedFrame, type Recording } from './recorder';
import { chainOfShown } from './chain-params';
import type { FlameRenderer, RendererOptions } from './render';
import type { VisualSettings } from './settings.svelte';

/** A pipeline as a replay drives it: a recorded frame drawn as its renderer takes it, and what the comparison reads. */
type Driver = {
  context: WebGL2RenderingContext;
  precision: number;
  draw(f: RecordedFrame, all: Recording): void;
  measure(w: number, h: number, t: number): void;
  tiles(): Tiles[];
  /** The frame's numbers into its meters (the Meter node, or meters.ts's over the frozen one's tiles): their values. */
  meter(stats: Omit<FrameStats, 'tiles'>): MeterValues;
  dispose(): void;
};
type Made = Pick<FlameRenderer, 'setPoints' | 'setFlame' | 'resize' | 'pointCount' | 'measure' | 'tiles' | 'dispose' | 'context' | 'precision'>;

/** What both renderers take alike before a frame: the points, the flame, the sizes. */
function prepare(r: Made, canvas: HTMLCanvasElement, f: RecordedFrame) {
  if (f.points !== r.pointCount) r.setPoints(f.points);
  r.setFlame(flameOf(f.flame));
  if (canvas.width !== f.w || canvas.height !== f.h) {
    canvas.width = f.w;
    canvas.height = f.h;
  }
  r.resize(f.bw, f.bh);
}

const driver = (r: Made, draw: Driver['draw'], meter: Driver['meter']): Driver => ({
  context: r.context,
  get precision() {
    return r.precision;
  },
  set precision(p: number) {
    r.precision = p as 32 | 16 | 8;
  },
  draw,
  measure: (w, h, t) => r.measure(w, h, t),
  tiles: () => r.tiles(),
  meter,
  dispose: () => r.dispose(),
});

/** Today's renderer. */
const today = async (canvas: HTMLCanvasElement, points: number, options: RendererOptions): Promise<Driver> => {
    const [{ FlameRenderer }, { v2Types }, { toV2 }, { cratarrTypes }, { DEFAULTS, RANGES }] = await Promise.all([import('./render'), import('./picture-graph'), import('./v2'), import('./patch'), import('./settings.svelte')]);
    const r = new FlameRenderer(canvas, points, 'high-performance', options);
    const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
    let patched = '';
    return driver(r, (f, all) => {
      prepare(r, canvas, f);
      // Today's chain with the extras the recording draws and the effects it draws now (each slot's at its place in
      // its engine's list).
      const ids = f.look.effects?.ids ?? [0, 0, 0];
      if (patched !== ids.join()) {
        const places = Object.fromEntries(ENGINES.map((e, k) => [`fx${k + 1}`, ENGINE_EFFECTS[e.key].findIndex((x) => x.id === ids[k]) + 1]));
        r.setPatch(toV2({ ...DEFAULTS, ...places, extra: all.frames.find((x) => x.compose)?.compose?.extra ?? emptyExtra() } as VisualSettings), types);
        if (r.errors.length) throw new Error(r.errors.join(' '));
        patched = ids.join();
      }
      if (f.pruned) r.prune();
      const c = f.compose;
      r.frame(f.passes, chainOfShown(lookOf(f.look)), f.w, f.h, c ? { extra: c.extra, knobsOf: (m) => c.knobs[m.id] ?? m.knobs, frame: frameOf(c), flame: c.flame, flameTo: c.flameTo } : null);
    }, (stats) => {
      r.meterAdd(stats);
      return r.meterValues;
    });
  };

/** The pipelines a replay can run: today's renderer (`current`: the patch's picture side as a graph, the nodes'
 *  values, its compose step the graph's own walk) and the frozen copies every step is compared with (`legacy`:
 *  render-legacy.ts and compose-legacy.ts, the look as one, the compose step as a hook). */
const PIPELINES: Record<string, (canvas: HTMLCanvasElement, points: number, options: RendererOptions) => Promise<Driver>> = {
  current: today,
  legacy: async (canvas, points, options) => {
    const [{ FlameRenderer }, { Compositor }] = await Promise.all([import('./render-legacy'), import('./compose-legacy')]);
    const r = new FlameRenderer(canvas, points, 'high-performance', options);
    const compositor = new Compositor(r.context);
    const meters = new Meters();
    const unread: Tiles[] = [];
    const tiles = r.tiles.bind(r);
    r.tiles = () => {
      const got = tiles();
      unread.push(...got);
      return got;
    };
    return driver(r, (f) => {
      prepare(r, canvas, f);
      if (f.pruned) compositor.prune(f.pruned);
      const c = f.compose;
      // A layer's Glow and Tint modules as the frozen one had them: its source's own knobs, at the blend.
      const own = new Map<string, Record<string, number>>();
      for (const l of c?.extra.layers ?? [])
        for (const m of l.modules) if (m.type === 'glow' || m.type === 'tint') own.set(l.source.id, { ...own.get(l.source.id), [m.type]: (c!.knobs[m.id] ?? m.knobs)[m.type] ?? 0 });
      const knobsOf = (m: Module) => ({ ...(c?.knobs[m.id] ?? m.knobs), ...own.get(m.id) });
      r.compose = c && Compositor.active(c.extra, knobsOf, c.flame) ? { active: true, run: (picture, width, height) => compositor.run(picture, width, height, c.extra, knobsOf, frameOf(c), c.flame) } : null;
      r.frame(f.passes, lookOf(f.look), f.w, f.h);
    }, (stats) => {
      meters.add({ ...stats, tiles: unread.splice(0) });
      return meters.now();
    });
  },
};

// ---- Replay ----

async function load(name: string): Promise<Recording> {
  const r = await fetch(`/tests/pictures/${name}.json.gz`);
  if (!r.ok) throw new Error(`No recording ${name}`);
  const bytes = new Uint8Array(await r.arrayBuffer());
  const text = bytes[0] === 0x1f && bytes[1] === 0x8b ? await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'))).text() : new TextDecoder().decode(bytes);
  return JSON.parse(text);
}

const SEED = 20261020;

/** `digest`: every frame's pixels hashed (FNV-1a), to tell two runs of the lab apart. */
type Side = { name: string; canvas: HTMLCanvasElement; renderer: Driver; tiles: Tiles[]; pixels: Uint8Array; digest: number };

function frameOf(c: RecordedCompose): FrameInfo {
  return { dt: c.frame.dt, time: c.frame.time, beat: c.frame.beat, signals: (ref) => c.frame.signals[signalKey(ref)] ?? 0, waveform: c.frame.waveform ? Float32Array.from(c.frame.waveform) : null };
}

async function replay(name: string, a: string, b: string, bits: 0 | 8 | 16 | 32, seedB = SEED) {
  const rec = await load(name);
  const clock = { at: 0 };
  const make = async (key: string, seed: number): Promise<Side> => {
    const canvas = document.createElement('canvas');
    canvas.style.cssText = 'width: 320px; height: auto; margin: 4px;';
    document.body.append(canvas);
    const p = PIPELINES[key];
    if (!p) throw new Error(`No pipeline ${key}`);
    const renderer = await p(canvas, rec.points, { random: randomFrom(seed), now: () => clock.at });
    if (bits) renderer.precision = bits;
    return { name: key, canvas, renderer, tiles: [], pixels: new Uint8Array(0), digest: 0x811c9dc5 };
  };
  const sides = [await make(a, SEED), await make(b, seedB)];
  // Each side's passes (its full-screen draws: the points' are not counted) over the frames.
  const passes = sides.map((s) => {
    const gl = s.renderer.context;
    const count = { n: 0 };
    const draw = gl.drawArrays.bind(gl);
    gl.drawArrays = (mode, first, n) => {
      if (mode === gl.TRIANGLES) count.n++;
      draw(mode, first, n);
    };
    return count;
  });
  const perFrame: number[] = [];
  let worst = 0;
  let worstAt = -1;
  let sum = 0;
  let squares = 0;
  let count = 0;
  let lit = 0;
  let metersWorst = 0;
  const started = performance.now();
  for (let i = 0; i < rec.frames.length; i++) {
    const f = rec.frames[i];
    clock.at = f.at;
    for (const s of sides) {
      const r = s.renderer;
      // The flash meter's readings first in a frame, as the visuals take them.
      s.tiles.push(...r.tiles());
      r.draw(f, rec);
      const gl = r.context;
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      if (s.pixels.length !== f.w * f.h * 4) s.pixels = new Uint8Array(f.w * f.h * 4);
      gl.readPixels(0, 0, f.w, f.h, gl.RGBA, gl.UNSIGNED_BYTE, s.pixels);
      let h = s.digest;
      for (let k = 0; k < s.pixels.length; k++) h = Math.imul(h ^ s.pixels[k], 0x01000193);
      s.digest = h >>> 0;
      r.measure(f.w, f.h, f.at / 1000);
    }
    // The frame's own numbers (stand-ins, the same for both): the meters' values compared, as the levers follow them.
    const stats = { t: f.at / 1000, turnRates: [0.4, -0.2, 0.1, 0, 0.3, -0.5], shape: [0.5, 0.6, 0.4, 0.5, 0.7, 0.3], hue: [0, 0.1, 0.2, 0.3, 0.4, 0.5], spread: [1, 1, 1, 1, 1, 1], scale: f.look.scale, saturation: f.look.saturation, look: null, symmetry: false };
    const [ma, mb] = sides.map((s) => s.renderer.meter(stats));
    for (const k of Object.keys(ma) as (keyof MeterValues)[]) if (typeof ma[k] === 'number') metersWorst = Math.max(metersWorst, Math.abs((ma[k] as number) - (mb[k] as number)));
    const [p, q] = sides.map((s) => s.pixels);
    let most = 0;
    for (let k = 0; k < p.length; k++) {
      if ((k & 3) === 3) continue;
      lit += p[k];
      const d = Math.abs(p[k] - q[k]);
      if (d > most) most = d;
      sum += d;
      squares += d * d;
    }
    count += (p.length / 4) * 3;
    perFrame.push(most);
    if (most > worst) [worst, worstAt] = [most, i];
    lab.progress = i + 1;
    await new Promise((r) => setTimeout(r, 0));
  }
  // The flash meter's tiles, matched by the time they were taken.
  const byT = new Map(sides[1].tiles.map((t) => [t.t, t]));
  let tilesWorst = 0;
  let tilesMatched = 0;
  for (const t of sides[0].tiles) {
    const u = byT.get(t.t);
    if (!u) continue;
    tilesMatched++;
    for (const key of ['luminance', 'u', 'v', 'red'] as const) for (let k = 0; k < t[key].length; k++) tilesWorst = Math.max(tilesWorst, Math.abs(t[key][k] - u[key][k]));
  }
  const mse = count ? squares / count : 0;
  const gl = sides[0].renderer.context;
  const info = gl.getExtension('WEBGL_debug_renderer_info');
  lab.result = {
    recording: name,
    a,
    b,
    frames: rec.frames.length,
    precision: sides.map((s) => s.renderer.precision),
    max: worst,
    worstFrame: worstAt,
    mean: count ? sum / count : 0,
    /** The first pipeline's mean value (of 255): a black picture is no test. */
    lit: count ? lit / count : 0,
    psnr: mse ? 10 * Math.log10((255 * 255) / mse) : Infinity,
    framesDiffering: perFrame.filter((d) => d > 0).length,
    /** The first frames' largest difference each. */
    first: perFrame.slice(0, 12),
    digests: sides.map((s) => s.digest.toString(16).padStart(8, '0')),
    tiles: { a: sides[0].tiles.length, b: sides[1].tiles.length, matched: tilesMatched, max: tilesWorst },
    /** The meters' largest difference over the frames (each value in its own units). */
    meters: metersWorst,
    /** Each side's passes a frame, on average. */
    passes: passes.map((c) => c.n / rec.frames.length),
    gpu: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : '?',
    seconds: (performance.now() - started) / 1000,
  };
  for (const s of sides) {
    s.renderer.dispose();
  }
}

const lab: { progress?: number; result?: Record<string, unknown>; error?: string } = {};
(window as unknown as { pictureLab: typeof lab }).pictureLab = lab;
window.addEventListener('error', (e) => (lab.error ??= e.message));
window.addEventListener('unhandledrejection', (e) => (lab.error ??= String(e.reason?.message ?? e.reason)));

const q = new URLSearchParams(location.search);
const run = q.get('replay')
  ? replay(q.get('replay')!, q.get('a') ?? 'current', q.get('b') ?? 'current', Number(q.get('bits') ?? 0) as 0 | 8 | 16 | 32, Number(q.get('seedB') ?? SEED))
  : Promise.resolve();
run.catch((e) => (lab.error = String(e?.message ?? e)));
