// Golden frames (0.10, the architecture audit's visuals step 1): what the frame loop (visuals-loop.ts) hands the
// renderer, frame by frame, for a rack over a recorded track: the chain's values (chainNow) and the Flame's camera where
// the rack has the Cratarr device, and for every module its knobs as the engine moves them, its node's values, its
// Field's flow and its Grid's points; every output and macro. Kept as references (tests/golden), so the engine's
// rework is proved to draw the same picture without a GPU. The loop's own pace, framing and meters are left out: they
// are the device's, not the rack's.
import { gunzipSync, gzipSync } from 'node:zlib';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { track, type Track } from './recorded.ts';

// The picture side's wiring installs itself into the conversion (v2.ts), as the app has it (snapshot-v2.ts).
await import('../../src/lib/visuals/picture-wiring.ts');
const { newEngine, stepEngine } = await import('../../src/lib/visuals/engine/graph.ts');
const { LivePatch } = await import('../../src/lib/visuals/engine/live-patch.ts');
const { rollOf } = await import('../../src/lib/visuals/engine/dice.ts');
const { musicOf, newMusicState } = await import('../../src/lib/visuals/music-core.ts');
const { newNormalState, signalsFor } = await import('../../src/lib/visuals/normal.ts');
const { T } = await import('../../src/lib/visuals/routes.ts');
const { BANDS, DEFAULTS, FLUID_FULL } = await import('../../src/lib/visuals/settings.svelte.ts');
const { applyKnobs, applyMacros, asSet, toV2 } = await import('../../src/lib/visuals/v2.ts');
const { v2TypeRecord } = await import('../../src/lib/visuals/v2-types.ts');
const { EXAMPLES } = await import('../../src/lib/visuals/examples.ts');
const { frameStats } = await import('../../src/lib/visuals/engine/frame-stats.ts');
const { stageIdsOf, stagesNow } = await import('../../src/lib/visuals/stage-values.ts');
const { v2Types } = await import('../../src/lib/visuals/picture-graph.ts');
const { cratarrTypes } = await import('../../src/lib/visuals/patch.ts');
const { RANGES } = await import('../../src/lib/visuals/settings.svelte.ts');
const { Meters, screenTile } = await import('../../src/lib/visuals/meters.ts');
const { CHAINS, chainRoutes } = await import('./oracle/chains.ts');
import type { PatchV2 } from '../../src/lib/visuals/ports.ts';
import type { MeterValues } from '../../src/lib/visuals/meters.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
export const GOLDEN = join(HERE, '..', 'golden');

/** How a rack is run: over which track, with reduced motion, its sidechains' loop closed through the meters (each frame
 *  measured, the meters' values back in four times a second, as the frame loop has them: the screen's flashes from
 *  the light the chain shows, `metered`). */
export type RunOptions = { track?: string; still?: boolean; metered?: boolean };
const settingsRack = (s: object) => () => toV2(structuredClone({ ...DEFAULTS, ...s }) as typeof DEFAULTS);
/** The racks the references cover: the Cratarr device at its defaults (the standard rack: over the groove, over the
 *  ambient track, with reduced motion, with every column's scale off 1, its sidechains closed through the meters), the
 *  preset chains' settings as an old snapshot converts them, and every example rack of the library (Fire among them).
 *  What the proofs on today's summary covered (the equivalence and sidechain tests, retired with it, 0.10). */
export const RACKS: { id: string; patch: () => PatchV2; run?: RunOptions }[] = [
  { id: 'standard', patch: settingsRack({}) },
  { id: 'standard-ambient', patch: settingsRack({}), run: { track: 'tr-8934' } },
  { id: 'standard-still', patch: settingsRack({}), run: { still: true } },
  { id: 'standard-scaled', patch: settingsRack({ shape: 1.4, rotation: 0.6, lights: 1.3, glow: 0.7, size: 0.55, saturation: 1.2, trails: 0.8, presence: 1.5, waves: 0.65 }) },
  { id: 'standard-metered', patch: settingsRack({}), run: { metered: true } },
  ...CHAINS.map((c) => ({ id: `chain-${c.id}`, patch: settingsRack({ ...(c.settings ?? {}), routes: chainRoutes(c), extra: c.extra }) })),
  ...EXAMPLES.map((x) => ({ id: x.id, patch: () => structuredClone(x.patch) })),
];
/** The groove (its beats in it), from half a minute in; every third frame of eight seconds at 30 fps kept. */
export const TRACK = 'tr-10329';
export const FROM = 30;
export const SECONDS = 8;
export const FPS = 30;
export const EVERY = 3;
/** The loop's blends (visuals-loop.ts). */
const BLEND_MS = 6000;
const ANOTHER_MS = 1500;

/** A value's numbers by path: objects and arrays walked; a long list of numbers (a Grid's points, a field) by its
 *  length, sum and sum of squares, so a reference stays small and still catches any change in it. */
function put(out: Map<string, number>, key: string, v: unknown) {
  if (typeof v === 'number') return void out.set(key, v);
  if (typeof v === 'boolean') return void out.set(key, v ? 1 : 0);
  if (!v || typeof v !== 'object') return;
  if (ArrayBuffer.isView(v) || (Array.isArray(v) && v.length > 24)) {
    const xs: number[] = [];
    const walk = (x: unknown) => {
      if (typeof x === 'number') xs.push(x);
      else if (x && typeof x === 'object') for (const y of Object.values(x as object)) walk(y);
    };
    walk(Array.isArray(v) ? v : Array.from(v as unknown as ArrayLike<number>));
    out.set(`${key}.n`, xs.length);
    out.set(`${key}.sum`, xs.reduce((a, b) => a + b, 0));
    out.set(`${key}.sq`, xs.reduce((a, b) => a + b * b, 0));
    return;
  }
  for (const [k, x] of Object.entries(v as object)) put(out, `${key}.${k}`, x);
}

/** About how bright the flame's dense parts show (relative luminance), for the closed loop's screen. */
const BRIGHT = 0.35;

/** The rack stepped as the frame loop steps it, over its track: each kept frame's numbers. */
export function goldenFrames(make: () => PatchV2, run: RunOptions = {}): Map<string, number>[] {
  const t: Track = track(run.track ?? TRACK);
  const meters = run.metered ? new Meters() : null;
  let meterValues: MeterValues | undefined;
  let meteredAt = -Infinity;
  const types = v2TypeRecord();
  // The renderer's types (Visuals.svelte's PICTURE_TYPES): which module of a type each stage reads.
  const pictureTypes = v2Types(cratarrTypes(RANGES, DEFAULTS));
  let patch = make();
  let ids = stageIdsOf(patch, pictureTypes);
  const live = new LivePatch();
  const engine = newEngine();
  const normal = newNormalState();
  const music = newMusicState();
  const dt = 1 / FPS;
  let easing: { from: Record<string, Record<string, number>>; to: Record<string, Record<string, number>>; start: number; ms: number } | null = null;
  let nextBlend: number | null = null;
  const frames: Map<string, number>[] = [];
  const isStep = (type: string, knob: string) => types[type]?.knobs.find((k) => k.id === knob.replace(/\.\d+$/, ''))?.kind === 'step';
  // Into the store (a new patch object, as its change and perform make), eased where it glides.
  const apply = (values: Record<string, Record<string, number>>, cables: Record<string, PatchV2['cables'][number]['value']>, ms: number, now: number, macros: Record<string, number> = {}) => {
    const flat = live.update(patch);
    const from: Record<string, Record<string, number>> = {};
    const to: Record<string, Record<string, number>> = {};
    for (const [id, knobs] of Object.entries(values)) {
      const m = flat.modules.find((x) => x.id === id);
      if (!m || m.type === 'flame') continue;
      const eased = Object.keys(knobs).filter((k) => !isStep(m.type, k));
      if (!eased.length) continue;
      from[id] = Object.fromEntries(eased.map((k) => [k, m.knobs[k]]));
      to[id] = Object.fromEntries(eased.map((k) => [k, knobs[k]]));
    }
    const next = structuredClone(patch);
    applyKnobs(next, values);
    for (const c of next.cables) if (c.id in cables) c.value = cables[c.id];
    applyMacros(next, macros);
    patch = next;
    if (ms > 0) easing = { from, to, start: now, ms };
  };
  for (let k = 0; k < SECONDS * FPS; k++) {
    const at = FROM + k * dt;
    const now = at * 1000;
    const flat = live.update(patch);
    if (easing) {
      const e = easing;
      const u = Math.min(1, (now - e.start) / e.ms);
      const s = u * u * (3 - 2 * u);
      for (const [id, knobs] of Object.entries(e.to)) {
        const m = flat.modules.find((x) => x.id === id);
        if (m) for (const [knob, v] of Object.entries(knobs)) m.knobs[knob] = e.from[id][knob] + (v - e.from[id][knob]) * s;
      }
      if (u >= 1) easing = null;
    }
    const ofType = (type: string) => flat.modules.find((x) => x.type === type);
    const kind = (type: string, knob: string) => types[type]?.knobs.find((x) => x.id === knob);
    const lighting = Array.from({ length: BANDS }, (_, b) => asSet(flat, ofType('music'), `hold.${b}`, types) ?? kind('music', 'hold')?.default ?? 0.15);
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: now, lighting });
    let signals: ReturnType<typeof signalsFor> | null = null;
    const read = (ref: Parameters<ReturnType<typeof signalsFor>>[0]) => {
      if (!signals) {
        m.track[T.track] = engine.signals[T.track];
        signals = signalsFor(normal, m, now);
      }
      return signals(ref);
    };
    const glide = nextBlend;
    nextBlend = null;
    const stepped = stepEngine(engine, flat, types, m, read, dt, {
      now,
      fluidFull: FLUID_FULL,
      still: !!run.still,
      meters: meterValues,
      aspect: 16 / 9,
      view: { cover: { key: m.key ?? null, colours: null }, glide },
    });
    const flameId = flat.modules.find((x) => x.type === 'flame')?.id;
    const fv = flameId ? stepped.flames[flameId] : undefined;
    const out = new Map<string, number>();
    const keep = k % EVERY === 0;
    if (keep) {
      for (const [id, knobs] of Object.entries(stepped.knobs)) put(out, `knobs.${id}`, knobs);
      for (const [id, p] of Object.entries(stepped.params)) put(out, `params.${id}`, p);
      for (const [id, f] of Object.entries(stepped.fields)) put(out, `fields.${id}`, f);
      for (const [id, ps] of Object.entries(stepped.points)) put(out, `points.${id}`, ps);
      for (const [id, v] of Object.entries(stepped.outputs)) put(out, `out.${id}`, v);
      for (const [id, v] of Object.entries(stepped.macros)) put(out, `macro.${id}`, v);
      out.set('nexts', stepped.nexts.length);
      out.set('dice', stepped.dice.length);
      if (stepped.device && fv) {
        put(out, 'camera', fv.camera);
        // What each stage draws with: the device's values, each stage's own modules' over them (the renderer's).
        put(out, 'chain', stagesNow(stepped.device.stages, ids, (id) => stepped.params[id]));
      }
    }
    // The meters' numbers (the frame loop's), and with the loop closed, the meters' values back in.
    const stats = frameStats(stepped, { patch: flat, t: at, look: null });
    if (keep && stats) put(out, 'stats', stats);
    if (meters && stats && stepped.device) {
      const show = stepped.device.stages.show;
      const lights = show.hits.reduce((a, h) => a + Math.max(0, h), 0) / Math.max(1, show.hits.length);
      meters.add({ ...stats, tiles: [screenTile(at, Math.min(1, BRIGHT * show.flash * (1 + 0.6 * lights)))] });
      if (now - meteredAt >= 250) {
        meterValues = meters.now();
        meteredAt = now;
      }
      if (keep && meterValues) put(out, 'meters', meterValues);
    }
    // What the loop does with the frame: a new track's look (the Dice) worked out and blended in, a step's Next moved on.
    for (const r of stepped.dice) {
      const g = rollOf(patch, types, r.seed, r.scope, r.reach);
      let n = g.next();
      while (!n.done) n = g.next();
      apply(n.value.values, n.value.cables, BLEND_MS, now, n.value.macros);
      nextBlend = BLEND_MS;
      break;
    }
    const moved: Record<string, Record<string, number>> = {};
    for (const r of stepped.nexts) (moved[r.module] ??= {})[r.knob] = r.to;
    if (Object.keys(moved).length) {
      apply(moved, {}, 0, now);
      nextBlend = ANOTHER_MS;
    }
    if (keep) frames.push(out);
  }
  return frames;
}

/** Rounded to 7 significant digits: the references' precision. */
const round = (v: number) => (Number.isFinite(v) && v !== 0 ? Number(v.toPrecision(7)) : v);

export const referenceFile = (id: string) => join(GOLDEN, `${id.replace(/[^a-z0-9-]+/gi, '-')}.json.gz`);
export function writeReference(id: string, frames: Map<string, number>[]) {
  const keys = [...new Set(frames.flatMap((f) => [...f.keys()]))].sort();
  const rows = frames.map((f) => keys.map((k) => (f.has(k) ? round(f.get(k)!) : null)));
  writeFileSync(referenceFile(id), gzipSync(JSON.stringify({ keys, rows })));
}
export function readReference(id: string): Map<string, number | null>[] | null {
  if (!existsSync(referenceFile(id))) return null;
  const { keys, rows } = JSON.parse(gunzipSync(readFileSync(referenceFile(id))).toString()) as { keys: string[]; rows: (number | null)[][] };
  return rows.map((r) => new Map(keys.map((k, i) => [k, r[i]])));
}

/** Where `got` departs from the reference: the worst relative difference (against the larger magnitude, 1 at least
 *  below 1), and every key the one has and the other has not. */
export function departures(ref: Map<string, number | null>[], got: Map<string, number>[]) {
  let worst = { d: 0, key: '', frame: -1, want: 0, got: 0 };
  const missing = new Set<string>();
  const extra = new Set<string>();
  if (ref.length !== got.length) return { frames: [ref.length, got.length] as const, worst, missing, extra };
  ref.forEach((r, i) => {
    const g = got[i];
    for (const [k, want] of r) {
      // Null: this frame had no such number (another frame had).
      if (want === null) {
        if (g.has(k)) extra.add(k);
        continue;
      }
      if (!g.has(k)) {
        missing.add(k);
        continue;
      }
      const v = g.get(k)!;
      const d = Number.isNaN(want) ? (Number.isNaN(v) ? 0 : Infinity) : Math.abs(v - want) / Math.max(1, Math.abs(want), Math.abs(v));
      if (d > worst.d) worst = { d, key: k, frame: i, want, got: v };
    }
    for (const k of g.keys()) if (!r.has(k)) extra.add(k);
  });
  return { frames: null, worst, missing, extra };
}
