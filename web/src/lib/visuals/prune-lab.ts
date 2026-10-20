// The prune lab (decision 0057; development only, prune-lab.html): fewer cables in a converted patch, each one judged
// by the picture it makes. The settings' v2 patch is replayed over recorded tracks (tests/fixtures/tracks), the one
// engine (engine/graph.ts stepEngine) stepped with the cables under test, its outcome drawn through the v2 picture graph
// (render.ts, chain-params.ts chainNow), as Visuals.svelte draws a frame; the screens compared with the whole patch's.
//
// Held as the whole patch has them, both sides alike: the look (its walk, its moments: the cables into them are kept,
// unjudged), the camera's framing (measured once), the sidechaining's levers (free), the colours' cover. The fluid's pushes
// follow the cables into Stir, Flow and Swirl (prune.ts stirGain), as Visuals gives them from the routes.
//
// window.pruneLab.input (set before the page loads): { settings, frames?, warm?, windows?, guard?, maxDiff?, minPsnr?,
// width?, height?, points?, shots? }; window.pruneLab.result: the outcome; .progress: where it is.
import { decode as decodeBeats, type Beats } from './beats';
import { chainNow } from './chain-params';
import { newEngine, stepEngine } from './engine/graph';
import { coloured, evolved, flameOf, measure, partsAt, PRESET_KEYS, randomFrom, type Flame, type Preset } from './flame';
import { FREE } from './engine/sidechain';
import { musicOf, newMusicState } from './music-core';
import { newNormalState, signalsFor } from './normal';
import { cratarrTypes, type ModuleType } from './patch';
import { v2Types } from './picture-graph';
import { ENGINE_MODULES, GRAPH_MODULES, showPartTypes, type PatchV2 } from './ports';
import { groupsOf, numbersOf, polyphonyMissing, pruned, stirGain, without, type Group } from './prune';
import { EXTRA_TYPES } from './registry';
import { A, B, T } from './routes';
import { DEFAULTS, FLUID_FULL, RANGES, type VisualSettings } from './settings.svelte';
import { decode as decodeSignals, type Signals } from './signals';
import { newStirring, stir } from './stirring';
import { flatten, toV2 } from './v2';
import './picture-wiring';

type Window = { track: string; from: number; seconds: number };
type Input = {
  settings: VisualSettings;
  /** The seconds drawn and compared, and the seconds stepped before each (the light and the engine settle). */
  windows?: Window[];
  warm?: number;
  /** The seconds the engine alone is stepped over to tell a cable that does nothing from one that acts elsewhere. */
  guard?: Window[];
  maxDiff?: number;
  minPsnr?: number;
  width?: number;
  height?: number;
  points?: number;
  passes?: number;
  /** The frames to keep as pictures (of the whole patch, and of the pruned one), by index in the compared frames. */
  shots?: number[];
};
export type Diff = { max: number; mean: number; psnr: number; frames: number; lit: number };

const FPS = 30;
const dt = 1 / FPS;
const chain = cratarrTypes(RANGES, DEFAULTS);
const PICTURE_TYPES = v2Types(chain);
const ENGINE_TYPES: Record<string, ModuleType> = Object.fromEntries([...EXTRA_TYPES, ...ENGINE_MODULES, ...GRAPH_MODULES, ...showPartTypes(chain.find((t) => t.type === 'show')!)].map((t) => [t.type, t]));

const tracks = new Map<string, { signals: Signals; beats: Beats | null }>();
async function trackOf(id: string) {
  if (!tracks.has(id)) {
    const get = async (ext: string) => (await fetch(`/tests/fixtures/tracks/${id}.${ext}`)).arrayBuffer();
    const signals = decodeSignals(await get('bands'));
    if (!signals) throw new Error(`${id}: no signals`);
    const raw = await get('beats');
    tracks.set(id, { signals, beats: raw.byteLength ? decodeBeats(raw) : null });
  }
  return tracks.get(id)!;
}

/** The flame the look's settings make, coloured as the Colours setting asks (no cover here). */
function baseFlame(s: VisualSettings): { flame: Flame; fire: number } {
  const mode = s.family >= 5 ? 'fire' : s.family >= 4 ? 'cover' : 'rainbow';
  const flame = coloured(flameOf(Object.fromEntries(PRESET_KEYS.map((k) => [k, s[k as keyof VisualSettings]])) as unknown as Preset, false), mode, null);
  return { flame, fire: mode === 'fire' ? 1 : 0 };
}

/** One run of `patch` (flattened) over a window: the engine's outcome each frame (`numbers`), and where `draw`, each
 *  compared frame's screen. */
async function run(s: VisualSettings, patch: PatchV2, w: Window, warm: number, draw: { width: number; height: number; points: number; passes: number; camera: Camera } | null) {
  const t = await trackOf(w.track);
  const st = newEngine();
  const music = newMusicState();
  const normal = newNormalState();
  const stirring = newStirring();
  const gain = stirGain(patch);
  const { flame: base, fire } = baseFlame(s);
  const frames: Uint8Array[] = [];
  const numbers: number[][] = [];
  let canvas: HTMLCanvasElement | null = null;
  let renderer: import('./render').FlameRenderer | null = null;
  let clock = 0;
  if (draw) {
    const { FlameRenderer } = await import('./render');
    canvas = document.createElement('canvas');
    canvas.width = draw.width;
    canvas.height = draw.height;
    document.body.append(canvas);
    renderer = new FlameRenderer(canvas, draw.points, 'high-performance', { random: randomFrom(20261020), now: () => clock });
    renderer.precision = 32;
    renderer.setPatch(toV2(structuredClone(s)), PICTURE_TYPES);
    if (renderer.errors.length) throw new Error(renderer.errors.join(' '));
    renderer.resize(draw.width, draw.height);
  }
  let partsWhere: ([number, number] | null)[] = [];
  let partsMeasured = -Infinity;
  const total = Math.round((warm + w.seconds) * FPS);
  const start = w.from - warm;
  for (let k = 0; k < total; k++) {
    const at = start + k * dt;
    const now = at * 1000;
    clock = now;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: now, lighting: s.lighting, key: 'lab' });
    const read = signalsFor(normal, m, now);
    const e = stepEngine(st, patch, ENGINE_TYPES, m, read, s, dt, { now, fluidFull: FLUID_FULL, levers: FREE });
    const d = e.aspects;
    // A converted patch always has the Cratarr chain.
    if (!d) throw new Error('no Cratarr chain in the patch');
    const compared = k >= Math.round(warm * FPS);
    if (compared) numbers.push(numbersOf(d));
    if (!renderer || !draw) continue;
    const drawn = evolved(base, d.parts, s.purity, s.spread);
    const cam = draw.camera;
    renderer.setFlame({ ...drawn, center: cam.center, size: cam.size });
    if (d.fluid > 0 && now - partsMeasured > 250) {
      partsMeasured = now;
      const aspect = draw.width / draw.height;
      const cos = Math.cos(d.turn);
      const sin = Math.sin(d.turn);
      partsWhere = partsAt(drawn).map((p) => {
        if (!p) return null;
        let x = ((p[0] - cam.center[0]) * d.scale) / cam.size;
        let y = ((p[1] - cam.center[1]) * d.scale) / cam.size;
        [x, y] = [cos * x - sin * y, sin * x + cos * y];
        if (aspect > 1) x /= aspect;
        else y *= aspect;
        return [x * 0.5 + 0.5, y * 0.5 + 0.5];
      });
    }
    const beatNow = m.playing ? m.beat : null;
    const stirred = stir(stirring, {
      t: now / 1000,
      dt,
      amount: d.fluid,
      parts: partsWhere,
      turns: d.rates,
      lights: d.hits,
      level: d.bands.map((v) => v[B.level]),
      continuity: s.continuity,
      hits: m.playing && m.hits ? m.hits.map((h) => (h.index < 0 ? null : { index: h.index, strength: h.weight, nextIn: h.nextIn })) : [],
      beat: beatNow ? { ...beatNow, nextIn: beatNow.length - beatNow.since } : null,
      downbeat: d.track[T.downbeat],
      section: d.track[T.section],
      loudness: d.lit,
      swell: d.track[T.swell],
      bps: m.bps,
      waves: d.given[A.waves],
      position: m.position,
      center: [0.5, 0.5],
      gain: (signal, band, aspect, x) => gain(signal, band, aspect, x),
    });
    const nodes = chainNow(d, {
      how: s,
      levers: FREE,
      ceiling: s.brightness,
      coverage: cam.coverage,
      fire,
      position: m.position,
      beat: beatNow,
      forces: stirred.forces,
      flow: stirred.flow,
      dt,
      speed: s.speed,
    });
    const extras = { extra: s.extra, knobsOf: (x: { id: string; knobs: Record<string, number> }) => e.knobs[x.id] ?? x.knobs, frame: { dt, time: now / 1000, beat: m.beat, signals: read, waveform: null }, flame: s.flameLayer ?? 1 };
    renderer.frame(draw.passes, nodes, draw.width, draw.height, extras as never);
    if (!compared) continue;
    const gl = renderer.context;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    const px = new Uint8Array(draw.width * draw.height * 4);
    gl.readPixels(0, 0, draw.width, draw.height, gl.RGBA, gl.UNSIGNED_BYTE, px);
    frames.push(px);
    renderer.measure(draw.width, draw.height, now / 1000);
    if (k % 10 === 0) await new Promise((r) => setTimeout(r, 0));
  }
  renderer?.dispose();
  canvas?.remove();
  return { frames, numbers };
}

type Camera = { center: [number, number]; size: number; coverage: number };

/** The difference of two runs' screens: the largest and mean (of 255, colour channels), the PSNR, and how lit `a` is. */
function diff(a: Uint8Array[], b: Uint8Array[]): Diff {
  let max = 0;
  let sum = 0;
  let squares = 0;
  let count = 0;
  let lit = 0;
  for (let f = 0; f < a.length; f++) {
    const p = a[f];
    const q = b[f];
    for (let k = 0; k < p.length; k++) {
      if ((k & 3) === 3) continue;
      const d = Math.abs(p[k] - q[k]);
      if (d > max) max = d;
      sum += d;
      squares += d * d;
      lit += p[k];
      count++;
    }
  }
  const mse = count ? squares / count : 0;
  return { max, mean: count ? sum / count : 0, psnr: mse ? 10 * Math.log10((255 * 255) / mse) : Infinity, frames: a.length, lit: count ? lit / count : 0 };
}

/** The largest difference of two runs' engine outcomes. */
function moved(a: number[][], b: number[][]): number {
  if (a.length !== b.length) return Infinity;
  let most = 0;
  for (let f = 0; f < a.length; f++) {
    if (a[f].length !== b[f].length) return Infinity;
    for (let k = 0; k < a[f].length; k++) {
      const [x, y] = [a[f][k], b[f][k]];
      if (Number.isNaN(x) && Number.isNaN(y)) continue;
      const d = Math.abs(x - y);
      most = Math.max(most, Number.isNaN(d) ? Infinity : d);
    }
  }
  return most;
}

/** Frames of two runs side by side, each column labelled (a PNG's data URL): the whole patch's and the pruned one's. */
function sheet(rows: { label: string; frames: Uint8Array[] }[], width: number, height: number): string {
  const pad = 8;
  const head = 28;
  const n = Math.max(...rows.map((r) => r.frames.length));
  const c = document.createElement('canvas');
  c.width = pad + n * (width + pad);
  c.height = rows.length * (head + height + pad) + pad;
  const g = c.getContext('2d')!;
  g.fillStyle = '#111';
  g.fillRect(0, 0, c.width, c.height);
  rows.forEach((r, i) => {
    const y = pad + i * (head + height + pad);
    g.fillStyle = '#eee';
    g.font = '600 18px system-ui, sans-serif';
    g.fillText(r.label, pad, y + 20);
    r.frames.forEach((px, k) => {
      const img = g.createImageData(width, height);
      // readPixels gives the rows bottom up.
      for (let row = 0; row < height; row++) img.data.set(px.subarray((height - 1 - row) * width * 4, (height - row) * width * 4), row * width * 4);
      for (let q = 3; q < img.data.length; q += 4) img.data[q] = 255;
      g.putImageData(img, pad + k * (width + pad), y + head);
    });
  });
  return c.toDataURL('image/png');
}

/** Fewer cables: the per-band copies merged, each cable's part in the picture measured with it gone, and dropped
 *  weakest first while the whole patch's picture stays within `maxDiff` (of 255) and `minPsnr` of the whole's. */
export async function pruneCables(v2: PatchV2, input: Input, report: (p: string) => void = () => {}) {
  const s = input.settings;
  const f = flatten(v2);
  const windows = input.windows ?? [
    { track: 'tr-10329', from: 30, seconds: 1.5 },
    { track: 'tr-21896', from: 40, seconds: 1.5 },
  ];
  const guard = input.guard ?? [
    { track: 'tr-10329', from: 10, seconds: 60 },
    { track: 'tr-21896', from: 10, seconds: 40 },
    { track: 'tr-8934', from: 10, seconds: 30 },
  ];
  const warm = input.warm ?? 0.5;
  const maxDiff = input.maxDiff ?? 2;
  const minPsnr = input.minPsnr ?? 45;
  const size = { width: input.width ?? 320, height: input.height ?? 180, points: input.points ?? 2 ** 15, passes: input.passes ?? 1 };
  const groups = groupsOf(f);
  // The camera, as the visuals frame the flame they start on.
  const first = await (async () => {
    const t = await trackOf(windows[0].track);
    const m = musicOf(t.signals, t.beats, windows[0].from, true, { state: newMusicState(), at: windows[0].from * 1000, lighting: s.lighting, key: 'lab' });
    const d = stepEngine(newEngine(), f, ENGINE_TYPES, m, signalsFor(newNormalState(), m, windows[0].from * 1000), s, dt, { now: windows[0].from * 1000, fluidFull: FLUID_FULL, levers: FREE }).aspects;
    if (!d) throw new Error('no Cratarr chain in the patch');
    const seen = measure(evolved(baseFlame(s).flame, d.parts, s.purity, s.spread));
    return seen ? { center: seen.center, size: seen.size, coverage: seen.coverage } : { center: [0, 0] as [number, number], size: 1, coverage: 0.33 };
  })();
  const draw = { ...size, camera: first };
  let renders = 0;
  const picture = async (p: PatchV2) => {
    renders++;
    const out: Uint8Array[] = [];
    for (const w of windows) out.push(...(await run(s, p, w, warm, draw)).frames);
    return out;
  };
  const engineOver = async (p: PatchV2, ws: Window[]) => {
    const out: number[][] = [];
    for (const w of ws) out.push(...(await run(s, p, w, 0, null)).numbers);
    return out;
  };
  report('the whole patch');
  const whole = await picture(f);
  const again = await picture(f);
  const self = diff(whole, again);
  if (self.max !== 0) throw new Error(`the replay is not the same twice (max ${self.max}/255): no judgement possible`);
  const wholeEngine = await engineOver(f, windows);
  const wholeGuard = await engineOver(f, guard);
  // Each cable: what the engine does without it, in the drawn seconds and over the guard's; drawn where it acts there.
  const judged: { group: Group; kind: 'silent' | 'elsewhere' | 'protected' | 'measured'; cost?: Diff; engine: number }[] = [];
  for (const [i, g] of groups.entries()) {
    report(`cable ${i + 1}/${groups.length}: ${g.id}`);
    if (g.protectedBy) {
      judged.push({ group: g, kind: 'protected', engine: NaN });
      continue;
    }
    const p = without(f, groups, new Set([g.id]));
    const inWindows = moved(wholeEngine, await engineOver(p, windows));
    const inGuard = moved(wholeGuard, await engineOver(p, guard));
    if (inWindows <= 1e-9 && inGuard <= 1e-9) judged.push({ group: g, kind: 'silent', engine: 0 });
    else if (inWindows <= 1e-9) judged.push({ group: g, kind: 'elsewhere', engine: inGuard });
    else judged.push({ group: g, kind: 'measured', cost: diff(whole, await picture(p)), engine: inWindows });
  }
  // Weakest first: those that do nothing, then by their mean difference (the largest, then the PSNR, to break ties).
  const order = judged
    .filter((j) => j.kind === 'silent' || j.kind === 'measured')
    .sort((a, b) => (a.cost?.mean ?? -1) - (b.cost?.mean ?? -1) || (a.cost?.max ?? -1) - (b.cost?.max ?? -1));
  const dropped = new Set<string>();
  const within = (d: Diff) => d.max <= maxDiff && d.psnr >= minPsnr;
  const silent = order.filter((j) => j.kind === 'silent').map((j) => j.group.id);
  if (silent.length) {
    report(`dropping the ${silent.length} that do nothing`);
    const d = diff(whole, await picture(without(f, groups, new Set(silent))));
    if (within(d)) for (const id of silent) dropped.add(id);
  }
  for (const j of order) {
    if (dropped.has(j.group.id)) continue;
    report(`trying without ${j.group.id}`);
    const next = new Set([...dropped, j.group.id]);
    const d = diff(whole, await picture(without(f, groups, next)));
    if (within(d)) dropped.add(j.group.id);
  }
  // The whole again, as pruned: the dropped cables deleted (kept beside it, for an undo).
  const { patch, deleted } = pruned(v2, f, groups, dropped);
  const last = await picture(flatten(patch));
  const final = diff(whole, last);
  const keep = groups.filter((g) => !dropped.has(g.id));
  const shots = (input.shots ?? []).filter((k) => k < whole.length);
  return {
    before: { groups: groups.length, cables: groups.reduce((n, g) => n + g.cables.length, 0) },
    after: { groups: keep.length, cables: keep.reduce((n, g) => n + g.cables.length, 0) },
    patch,
    deleted,
    keep: keep.map((g) => ({ id: g.id, merged: g.merged, cables: g.cables.map((c) => c.id), why: judged.find((j) => j.group === g)?.kind, protectedBy: g.protectedBy, cost: judged.find((j) => j.group === g)?.cost })),
    drop: judged.filter((j) => dropped.has(j.group.id)).map((j) => ({ cable: j.group.id, merged: j.group.merged, cables: j.group.cables.map((c) => c.id), cost: j.cost ?? null, kind: j.kind })),
    merged: groups.filter((g) => g.merged).map((g) => ({ id: g.id, of: g.cables.map((c) => c.id), missing: polyphonyMissing(g, f, PICTURE_TYPES) })),
    diff: final,
    self,
    renders,
    camera: first,
    sheet: shots.length
      ? sheet(
          [
            { label: `Before: ${groups.reduce((n, g) => n + g.cables.length, 0)} cables`, frames: shots.map((k) => whole[k]) },
            { label: `After: ${keep.reduce((n, g) => n + g.cables.length, 0)} cables${keep.some((g) => g.merged) ? ` (${keep.length} with each route's six bands as one)` : ''}`, frames: shots.map((k) => last[k]) },
          ],
          size.width,
          size.height,
        )
      : null,
    shotFrames: shots,
    size,
    windows,
    guard,
  };
}

const lab: { input?: Input; progress?: string; result?: unknown; error?: string } = (window as unknown as { pruneLab?: typeof lab }).pruneLab ?? {};
(window as unknown as { pruneLab: typeof lab }).pruneLab = lab;
window.addEventListener('error', (e) => (lab.error ??= e.message));
window.addEventListener('unhandledrejection', (e) => (lab.error ??= String(e.reason?.message ?? e.reason)));
if (lab.input) {
  const s = lab.input.settings;
  pruneCables(toV2(structuredClone(s)), lab.input, (p) => (lab.progress = p)).then(
    (r) => (lab.result = r),
    (e) => (lab.error = String(e?.message ?? e)),
  );
}
