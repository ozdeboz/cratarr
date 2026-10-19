// The picture lab (decision 0057, step 3; development only, picture-lab.html): the picture-diff harness the picture
// graph's steps are proved by, frame by frame against today's renderer.
//   ?record=<scene>&frames=N   the real visuals over a stand-in track (its beats and signals made here) in one of the
//                              scenes' settings, each frame's inputs recorded (recorder.ts)
//   ?replay=<name>&a=<pipeline>&b=<pipeline>&bits=32|16|8   a recording (tests/pictures/<name>.json.gz) through two
//                              pipelines, the same points and clock for both, each frame's screen read back and
//                              compared: the largest and mean difference (of 255), the PSNR, and the flash meter's tiles;
//                              &seedB=n starts the second one's points elsewhere (the harness's own check: it differs)
// window.pictureLab holds the recording or the result; web/scripts/check-picture.mjs drives both.
import { EXAMPLES } from './chains';
import { randomFrom } from './flame';
import type { Tiles } from './meters';
import type { FrameInfo, Module, PatchExtra } from './patch';
import { emptyExtra } from './patch';
import { flameOf, lookOf, signalKey, type RecordedCompose, type RecordedFrame, type Recording, type RecordRequest } from './recorder';
import { chainOfShown } from './chain-params';
import type { FlameRenderer, RendererOptions, Shown } from './render';
import type { VisualSettings } from './settings.svelte';
import { ID } from './signal-ids';

/** A pipeline as a replay drives it: a recorded frame drawn as its renderer takes it, and what the comparison reads. */
type Driver = {
  context: WebGL2RenderingContext;
  precision: number;
  draw(f: RecordedFrame, all: Recording): void;
  measure(w: number, h: number, t: number): void;
  tiles(): Tiles[];
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

const driver = (r: Made, draw: Driver['draw']): Driver => ({
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
  dispose: () => r.dispose(),
});

/** The pipelines a replay can run: today's renderer (`current`: the patch's picture side as a graph, the nodes'
 *  values, its own compose step) and the frozen copies every step is compared with (`legacy`: render-legacy.ts and
 *  compose-legacy.ts, the look as one, the compose step as a hook). */
const PIPELINES: Record<string, (canvas: HTMLCanvasElement, points: number, options: RendererOptions) => Promise<Driver>> = {
  current: async (canvas, points, options) => {
    const [{ FlameRenderer }, { v2Types }, { toV2 }, { cratarrTypes }, { DEFAULTS, RANGES }] = await Promise.all([import('./render'), import('./picture-graph'), import('./v2'), import('./patch'), import('./settings.svelte')]);
    const r = new FlameRenderer(canvas, points, 'high-performance', options);
    const types = v2Types(cratarrTypes(RANGES, DEFAULTS));
    let patched = false;
    return driver(r, (f, all) => {
      prepare(r, canvas, f);
      if (!patched) {
        // Today's chain with the extras the recording draws.
        r.setPatch(toV2({ ...DEFAULTS, extra: all.frames.find((x) => x.compose)?.compose?.extra ?? emptyExtra() } as VisualSettings), types);
        if (r.errors.length) throw new Error(r.errors.join(' '));
        patched = true;
      }
      if (f.pruned) r.prune(f.pruned);
      const c = f.compose;
      r.frame(f.passes, chainOfShown(lookOf(f.look)), f.w, f.h, c ? { extra: c.extra, knobsOf: (m) => c.knobs[m.id] ?? m.knobs, frame: frameOf(c), flame: c.flame } : null);
    });
  },
  legacy: async (canvas, points, options) => {
    const [{ FlameRenderer }, { Compositor }] = await Promise.all([import('./render-legacy'), import('./compose-legacy')]);
    const r = new FlameRenderer(canvas, points, 'high-performance', options);
    const compositor = new Compositor(r.context);
    return driver(r, (f) => {
      prepare(r, canvas, f);
      if (f.pruned) compositor.prune(f.pruned);
      const c = f.compose;
      const knobsOf = (m: Module) => c?.knobs[m.id] ?? m.knobs;
      r.compose = c ? { active: true, run: (picture, width, height) => compositor.run(picture, width, height, c.extra, knobsOf, frameOf(c), c.flame) } : null;
      r.frame(f.passes, lookOf(f.look), f.w, f.h);
    });
  },
};

// ---- Scenes: the settings a recording is made in (the look's own pinned, so the track's preset leaves them) ----

const example = (id: string) => EXAMPLES.find((c) => c.id === id)!.extra;
const FX = ['fx1', 'fx1Amount', 'fx1K1', 'fx1K2', 'fx2', 'fx2Amount', 'fx2K1', 'fx2K2', 'fx3', 'fx3Amount', 'fx3K1', 'fx3K2', 'fxLoop', 'fxLoopAmount', 'fxLoopK1', 'fxLoopK2'];
const layered: PatchExtra = {
  ...emptyExtra(),
  layers: [...example('example-kaleido-layer').layers, ...example('example-window').layers],
  loop: example('example-feedback').loop,
};
export const SCENES: Record<string, Partial<VisualSettings>> = {
  plain: {},
  fluid: { fluid: 0.8, facets: 0.6, lattice: 1, pins: ['facets', 'lattice', ...FX] },
  tunnel: { tunnel: 0.6, effects: 1, fxLoop: 1, fxLoopAmount: 0.8, pins: FX },
  dive: { dive: 0.6, pins: FX },
  // Rotate (Kaleidoscope's third), Pulse (EYEDO's first) and Echo (Hydra's first).
  effects: { effects: 1, fx1: 3, fx1Amount: 1, fx2: 1, fx2Amount: 0.8, fx3: 1, fx3Amount: 0.8, pins: FX },
  layers: { extra: layered, pins: FX },
};

// ---- The stand-in track: 120 beats a minute, a bar of four, six bands' hits and levels, sections every 16 bars ----

const BPM = 120;
const LENGTH = 600;
const beatTimes = Array.from({ length: (LENGTH * BPM) / 60 }, (_, i) => (i * 60) / BPM);

function standInBeats(): ArrayBuffer {
  const v = new DataView(new ArrayBuffer(beatTimes.length * 5));
  beatTimes.forEach((t, i) => {
    v.setUint32(i * 5, Math.round(t * 1000), true);
    v.setUint8(i * 5 + 4, i % 4 === 0 ? 255 : 150);
  });
  return v.buffer;
}

/** The server's form (signals.ts `decode`), made up: each band hits on its own part of the bar. */
function standInSignals(): ArrayBuffer {
  const rnd = randomFrom(57);
  const parts: number[][] = [];
  let sections = 0;
  const header = (id: number, kind: number, rate: number, channels: number, count: number, scale = 255) => {
    sections++;
    parts.push([id, kind, (rate * 10) & 255, (rate * 10) >> 8, channels, count & 255, (count >> 8) & 255, (count >> 16) & 255, count >>> 24, scale]);
  };
  const u32 = (n: number) => [n & 255, (n >> 8) & 255, (n >> 16) & 255, n >>> 24];
  const events = (at: number[], value: (i: number) => number) => at.flatMap((t, i) => [...u32(Math.round(t * 1000)), Math.round(255 * value(i))]);
  // Each band's hits: the sub on the bar's one and three, the bass on every beat, the mids on the off-beats, the air on
  // every eighth; a break of four bars in every sixteen.
  const step = 60 / BPM;
  const inBreak = (t: number) => Math.floor(t / (step * 4)) % 16 >= 12;
  const hitsOf = (b: number) =>
    beatTimes
      .flatMap((t, i) => (b === 0 ? (i % 2 === 0 ? [t] : []) : b === 1 ? [t] : b === 5 ? [t, t + step / 2] : b % 2 ? [t + step / 2] : i % 2 ? [t] : []))
      .filter((t) => !inBreak(t));
  const hits = Array.from({ length: 6 }, (_, b) => hitsOf(b));
  const RATE = 20;
  const near = (list: number[], t: number) => {
    let best = Infinity;
    for (const x of list) if (x <= t && t - x < best) best = t - x;
    return best;
  };
  const series = (id: number, channels: number, value: (t: number, c: number) => number, rate = RATE) => {
    const n = LENGTH * rate;
    header(id, 0, rate, channels, n);
    const data: number[] = [];
    for (let i = 0; i < n; i++) for (let c = 0; c < channels; c++) data.push(Math.max(0, Math.min(255, Math.round(255 * value(i / rate, c)))));
    parts.push(data);
  };
  const ring = (b: number, t: number) => Math.exp(-near(hits[b], t) / (0.08 + 0.04 * (5 - b)));
  const swell = (t: number) => 0.5 + 0.5 * Math.sin((2 * Math.PI * t) / (step * 64));
  header(ID.table, 3, 0, 0, 6);
  parts.push([20, 60, 60, 250, 250, 500, 500, 2000, 2000, 6000, 6000, 0].flatMap((hz) => [hz & 255, hz >> 8]));
  series(ID.loudness, 1, (t) => (inBreak(t) ? 150 : 170 + 10 * swell(t)) / 255, 10);
  series(ID.level, 6, (t, b) => 0.3 + 0.5 * ring(b, t) + 0.1 * rnd());
  series(ID.flux, 6, (t, b) => 0.7 * ring(b, t) + 0.1 * rnd());
  series(ID.percussive, 6, (t, b) => 0.6 * ring(b, t));
  series(ID.harmonic, 6, (t, b) => 0.4 + 0.3 * swell(t + b));
  series(ID.crest, 6, (t, b) => 0.3 + 0.4 * ring(b, t));
  series(ID.width, 6, (_, b) => 0.3 + 0.1 * b);
  series(ID.bandSwell, 6, (t, b) => swell(t + b * 0.5));
  series(ID.centroid, 1, (t) => 0.4 + 0.2 * swell(t * 2));
  series(ID.chroma, 12, (t, c) => (c === Math.floor(t / (step * 16)) % 12 ? 0.9 : 0.2));
  series(ID.harmonicChange, 1, (t) => (t % (step * 16) < 0.5 ? 0.8 : 0.1));
  series(ID.swell, 1, swell);
  series(ID.buildUp, 1, (t) => (Math.floor(t / (step * 4)) % 16 >= 10 && !inBreak(t) ? 0.8 : 0.1));
  series(ID.novelty, 1, (t) => (t % (step * 64) < 1 ? 0.9 : 0.1), 2);
  // A bar's start (0) on every fourth beat, sure of it.
  header(ID.bar, 0, 0, 1, beatTimes.length, 200);
  parts.push(beatTimes.map((_, i) => (i % 4 === 0 ? 0 : 64 * (i % 4))));
  for (const [id, list] of [
    [ID.hits, hits],
    [ID.percussiveHits, hits.map((h, b) => (b < 3 ? h : []))],
  ] as const) {
    header(id, 1, 0, 6, 0);
    for (const h of list) parts.push([...u32(h.length), ...events(h, (i) => 0.5 + 0.5 * ((i % 4) === 0 ? 1 : rnd()))]);
  }
  const starts = Array.from({ length: Math.floor(LENGTH / (step * 64)) }, (_, i) => i * step * 64);
  header(ID.boundaries, 2, 0, 0, starts.length);
  parts.push(events(starts, () => 1));
  const changes = Array.from({ length: Math.floor(LENGTH / (step * 16)) }, (_, i) => i * step * 16);
  header(ID.harmonicChanges, 2, 0, 0, changes.length);
  parts.push(events(changes, () => 0.8));
  return Uint8Array.from([3, sections, ...parts.flat()]).buffer;
}

/** The API as the visuals ask it, answered here: the stand-in track's beats, signals and tempo; nothing else. */
function standInApi() {
  const real = window.fetch.bind(window);
  const beats = standInBeats();
  const signals = standInSignals();
  window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url, location.href);
    if (!url.pathname.startsWith('/api/')) return real(input, init);
    if (url.pathname.endsWith('/bands')) return new Response(signals.slice(0));
    if (url.pathname.endsWith('/beats')) return new Response(beats.slice(0));
    if (url.pathname.startsWith('/api/track/') && url.pathname.endsWith('/playlists'))
      return Response.json({ addable: false, album_id: null, favorite: null, playlists: [], info: { bpm: BPM } });
    return new Response('', { status: 404 });
  };
}

async function record(scene: string, max: number) {
  const settings = SCENES[scene];
  if (!settings) throw new Error(`No scene ${scene}`);
  // The fewest points and steps (SwiftShader draws on the CPU), thirty frames a second.
  localStorage.setItem('cratarr-visuals', JSON.stringify({ quality: 0, fps: 30, ...settings }));
  standInApi();
  const request: RecordRequest = { name: scene, max };
  (globalThis as { cratarrPictureRecord?: RecordRequest }).cratarrPictureRecord = request;
  lab.request = request;
  const { local } = await import('../localPlayer.svelte');
  local.queue = [{ source: 'cratarr', id: 'tr-lab', name: 'Lab', artist: 'Lab', album: null, album_id: null } as never];
  local.index = 0;
  local.duration = LENGTH;
  local.paused = false;
  const t0 = performance.now();
  setInterval(() => {
    local.position = (performance.now() - t0) / 1000;
    local.paused = false;
  }, 2);
  const { mount } = await import('svelte');
  const { default: Visuals } = await import('./Visuals.svelte');
  mount(Visuals, { target: document.getElementById('app')!, props: { onclose: () => {} } });
}

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
  const perFrame: number[] = [];
  let worst = 0;
  let worstAt = -1;
  let sum = 0;
  let squares = 0;
  let count = 0;
  let lit = 0;
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
    digests: sides.map((s) => s.digest.toString(16).padStart(8, '0')),
    tiles: { a: sides[0].tiles.length, b: sides[1].tiles.length, matched: tilesMatched, max: tilesWorst },
    gpu: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : '?',
    seconds: (performance.now() - started) / 1000,
  };
  for (const s of sides) {
    s.renderer.dispose();
  }
}

const lab: { request?: RecordRequest; progress?: number; result?: Record<string, unknown>; error?: string } = {};
(window as unknown as { pictureLab: typeof lab }).pictureLab = lab;
window.addEventListener('error', (e) => (lab.error ??= e.message));
window.addEventListener('unhandledrejection', (e) => (lab.error ??= String(e.reason?.message ?? e.reason)));

const q = new URLSearchParams(location.search);
const run = q.get('record')
  ? record(q.get('record')!, Number(q.get('frames') ?? 240))
  : q.get('replay')
    ? replay(q.get('replay')!, q.get('a') ?? 'current', q.get('b') ?? 'current', Number(q.get('bits') ?? 0) as 0 | 8 | 16 | 32, Number(q.get('seedB') ?? SEED))
    : Promise.resolve();
run.catch((e) => (lab.error = String(e?.message ?? e)));
