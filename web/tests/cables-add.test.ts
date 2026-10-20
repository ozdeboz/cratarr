// Cables only add (decision 0057 §7e): a level column's cables as VCAs into its gain input, a moment column's through
// a Merge (largest) and a Beat gate into its next, a band's cables into another column through a Merge (mean). The
// engine over a patch so converted gives what it gave over the patch as it was, frame by frame (and that, the
// equivalence harness holds to today's).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, track, type Engine } from './harness/aspects-record.ts';

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { ASPECTS, MOMENT_ASPECTS, routeKey } = await import('../src/lib/visuals/routes.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;

/** The engine over today's settings converted: cables only adding (toV2 as it ships) or as before that step. */
const engineOver = (addOnly: boolean): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s) {
        from = s;
        p = flatten(toV2(s, { addOnly }));
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
  const p = flatten(toV2(busy));
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
      const a = record(engineOver(false), t, s, 30, 30, false);
      const b = record(engineOver(true), t, s, 30, 30, false);
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
