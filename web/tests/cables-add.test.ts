// Cables only add (decision 0057 §7e): a level column's cables as VCAs into its gain input, a moment column's through
// a Merge (largest) and a Beat gate into its next, a band's cables into another column through a Merge (mean). The
// engine over a patch so converted gives what it gave over the patch as it was, frame by frame (and that, the
// equivalence harness holds to today's).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, track, type Engine } from './harness/aspects-record.ts';

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngine, stepEngine, columnAt } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { shapeOf } = await import('../src/lib/visuals/evaluate.ts');
const { ASPECTS, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, routeKey } = await import('../src/lib/visuals/routes.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;
type CableV2 = import('../src/lib/visuals/ports.ts').CableV2;
type Module = import('../src/lib/visuals/patch.ts').Module;

/** The conversion, as toV2 is to write it (claude-5f's shapes to claude-a5). */
function cablesAdd(p: PatchV2): PatchV2 {
  const modules: Module[] = [...p.modules];
  const cables: CableV2[] = [];
  const byColumn = new Map<string, CableV2[]>();
  for (const c of p.cables) {
    const key = `${c.to.module}|${c.to.port}`;
    const a = columnAt(key);
    const aspect = a === undefined ? null : ASPECTS[a];
    const level = aspect !== null && LEVEL_ASPECTS.includes(aspect);
    const moment = aspect !== null && (MOMENT_ASPECTS as readonly string[]).includes(aspect);
    const merged = aspect !== null && !level && !moment && !PART_ASPECTS.includes(aspect) && c.from.channel !== undefined;
    if (level || moment || merged) (byColumn.get(key) ?? byColumn.set(key, []).get(key)!).push(c);
    else cables.push(c);
  }
  for (const [key, cs] of byColumn) {
    const [module, knob] = key.split('|');
    const col = key.replace(/[|/]/g, '-');
    const aspect = ASPECTS[columnAt(key)!];
    const one = (from: string, to: CableV2['to'], id: string): CableV2 => ({ id, from: { module: from, port: 'out' }, to, value: 1, mode: 'swing', depth: null });
    if (LEVEL_ASPECTS.includes(aspect)) {
      let prev: string | null = null;
      let i = 0;
      const vca = (c: CableV2) => {
        const id = `vca-${col}-${i++}`;
        const peak = shapeOf(c.value).peak;
        const ch = c.from.channel !== undefined ? { channel: c.from.channel } : {};
        modules.push({ id, type: 'vca', knobs: { gain: c.depth === null ? 1 - peak : 1 } });
        if (prev) cables.push(one(prev, { module: id, port: 'in' }, `${id}-in`));
        cables.push({ ...c, mode: 'swing', to: { module: id, port: 'cv', ...ch } });
        if (c.depth !== null) cables.push({ id: `${id}-peak`, from: c.depth ?? { module: 'envelope', port: 'out' }, to: { module: id, port: 'cv', ...ch }, value: -peak, mode: 'swing', depth: null });
        prev = id;
      };
      const bands = cs.filter((c) => c.from.channel !== undefined);
      for (const c of bands) vca(c);
      if (bands.length) {
        modules.push({ id: `merge-${col}`, type: 'merge', knobs: { mode: 3 } });
        cables.push(one(prev!, { module: `merge-${col}`, port: 'in' }, `merge-${col}-in`));
        prev = `merge-${col}`;
      }
      for (const c of cs.filter((c) => c.from.channel === undefined)) vca(c);
      cables.push(one(prev!, { module, port: `${knob}:gain` }, `${col}-gain`));
    } else if ((MOMENT_ASPECTS as readonly string[]).includes(aspect)) {
      modules.push({ id: `merge-${col}`, type: 'merge', knobs: { mode: 2 } });
      for (const c of cs) cables.push({ ...c, to: { module: `merge-${col}`, port: 'in', ...(c.from.channel !== undefined ? { channel: c.from.channel } : {}) } });
      modules.push({ id: `gate-${col}`, type: 'gate', knobs: { above: 0.5, hysteresis: 1 / 3, lift: 0, recover: 0, release: 0, gap: 2, amount: 1 } });
      cables.push(one(`merge-${col}`, { module: `gate-${col}`, port: 'in' }, `gate-${col}-in`));
      cables.push(one(`gate-${col}`, { module, port: `${knob}:next` }, `${col}-next`));
    } else {
      modules.push({ id: `mean-${col}`, type: 'merge', knobs: { mode: 1 } });
      for (const c of cs) cables.push({ ...c, to: { module: `mean-${col}`, port: 'in', channel: c.from.channel } });
      cables.push(one(`mean-${col}`, { module, port: knob }, `mean-${col}-out`));
    }
  }
  return { ...p, modules, cables };
}

const engineOver = (convert: (p: PatchV2) => PatchV2): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s) {
        from = s;
        p = convert(flatten(toV2(s)));
      }
      return stepEngine(st, p!, {}, m, () => 0, s, dt, opts).aspects;
    },
  };
};

/** The Cratarr chain with cables of every kind the conversion takes: levels by band and whole (the loudness's
 *  undepthed among them), moments by band and whole, a band's swing into another column. */
const busy = structuredClone(DEFAULTS);
busy.routes = {
  ...busy.routes,
  [routeKey('level', 2, 'tunnel')]: 0.8,
  [routeKey('flux', 4, 'tunnel')]: 0.5,
  [routeKey('beat', null, 'tunnel')]: 0.4,
  [routeKey('loudness', null, 'haze')]: 0.6,
  [routeKey('level', 1, 'fluid')]: 0.7,
  [routeKey('beat', null, 'symmetry')]: 0.7,
  [routeKey('hits', 1, 'symmetry')]: 0.9,
  [routeKey('percussive', 3, 'family')]: 1,
  [routeKey('flux', 2, 'brightness')]: 0.7,
  [routeKey('level', 5, 'saturation')]: 0.4,
};

test('the conversion has what it says: VCAs into gains, a Merge and a Gate into nexts, a Merge into a column', () => {
  const p = cablesAdd(flatten(toV2(busy)));
  const types = new Set(p.modules.map((m) => m.type));
  for (const t of ['vca', 'merge', 'gate']) assert.ok(types.has(t), t);
  assert.ok(p.cables.some((c) => c.to.port.endsWith(':gain')));
  assert.ok(p.cables.some((c) => c.to.port.endsWith(':next')));
  assert.ok(!p.cables.some((c) => c.mode === 'level'), 'no level cable left');
});

const MOMENT_GIVEN = new Set(MOMENT_ASPECTS.map((a) => `given.${ASPECTS.indexOf(a)}`));

test('cables only add: the engine over the converted patch gives what it gave, frame by frame, on every recorded track', () => {
  const variants: [string, typeof busy][] = [
    ['the Cratarr chain', structuredClone(DEFAULTS)],
    ['every kind of cable', busy],
  ];
  for (const id of TRACKS) {
    const t = track(id);
    for (const [name, s] of variants) {
      const a = record(engineOver((p) => p), t, s, 30, 30, false);
      const b = record(engineOver(cablesAdd), t, s, 30, 30, false);
      for (const [k, d] of compare(a, b)) {
        // A moment column's own sum is the Merge's now (what it gives is the Gate's firing, compared with the rest).
        if (MOMENT_GIVEN.has(k)) continue;
        if (d.most > 1e-6 && process.env.TRACE) {
          const fires = (f: Map<string, number>[]) => f.map((x, i) => (x.get(k) ? i : -1)).filter((i) => i >= 0).join(',');
          console.log(`TRACE ${k}: before ${fires(a)} | after ${fires(b)}`);
        }
        // Within the 32-bit floats the columns' sums are kept in (a product rounded once, not as it goes).
        assert.ok(d.most <= 1e-6, `${id} ${name}: ${k} differs by ${d.most} at frame ${d.frame}`);
      }
    }
  }
});
