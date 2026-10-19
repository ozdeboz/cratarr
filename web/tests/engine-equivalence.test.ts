// The one engine against today's (decision 0057, step 2), frame by frame over recorded tracks. Until the one engine
// runs, today's against itself: the recording is whole and the same every run, on every track.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, todays, track, type Engine } from './harness/aspects-record.ts';
const { newEngineV1, stepEngineV1 } = await import('../src/lib/visuals/engine/v1.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { ASPECT_KNOB } = await import('../src/lib/visuals/patch.ts');
const { LEVEL_ASPECTS } = await import('../src/lib/visuals/routes.ts');
import type { PatchV2, CableV2 } from '../src/lib/visuals/ports.ts';

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const settings = structuredClone(DEFAULTS);

test('the recorded tracks decode, the groove with its beats', () => {
  const ts = TRACKS.map(track);
  assert.ok(ts.every((t) => t.signals.series.size > 5));
  assert.ok(ts[0].beats && ts[0].beats.at.length > 100);
});

test("today's engine, recorded twice, is the same frame for frame: the harness itself", () => {
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(todays(), t, settings, 30, 10);
    const b = record(todays(), t, settings, 30, 10);
    assert.equal(a.length, 300);
    assert.ok(a[0].size > 100, `${a[0].size} numbers a frame`);
    for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} ${k} at frame ${d.frame}`);
    // It moves: the music is in it.
    const moved = [...compare(a, a.map(() => a[0]))].filter(([, d]) => d.most > 1e-3).length;
    assert.ok(moved > 10, `${id}: ${moved} fields move`);
  }
});

/** The one engine's modules, given by today's routes. */
const modules = (): Engine => {
  const st = newEngineV1();
  return { name: 'engine v1', step: (m, s, dt, opts) => stepEngineV1(st, m, s, dt, opts) };
};

test("the hidden behaviours as modules (Envelope, Clock, Track, Flash, the chain's parts) give today's picture exactly", () => {
  for (const id of TRACKS) {
    const t = track(id);
    for (const from of [0, 60]) {
      const a = record(todays(), t, settings, from, 20);
      const b = record(modules(), t, settings, from, 20);
      for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} from ${from} s: ${k} differs by ${d.most} at frame ${d.frame}`);
    }
  }
});

test('the same with reduced motion, and under each preset chain', async () => {
  const { CHAINS, chainRoutes } = await import('../src/lib/visuals/chains.ts');
  const variants: [string, typeof settings, boolean][] = [['reduced motion', settings, true]];
  for (const c of CHAINS) {
    if (c.extra.cables.length || c.extra.modulators.length) continue;
    variants.push([c.id, { ...settings, ...(c.settings ?? {}), routes: chainRoutes(c) } as typeof settings, false]);
  }
  for (const id of [TRACKS[0], TRACKS[3]]) {
    const t = track(id);
    for (const [name, s, still] of variants) {
      const a = record(todays(), t, s, 30, 15, still);
      const b = record(modules(), t, s, 30, 15, still);
      for (const [k, d] of compare(a, b)) assert.equal(d.most, 0, `${id} ${name}: ${k} differs by ${d.most} at frame ${d.frame}`);
    }
  }
});

/** Today's routes as a v2 patch, by the conversion's contract (a5's conversion takes its place when it lands): the
 *  loudness from the Envelope's level, undepthed; the Track's pulse; every other signal from Music, read as it is. */
function v2Of(routes: Record<string, unknown>): PatchV2 {
  const cables: CableV2[] = [];
  for (const [key, value] of Object.entries(routes)) {
    if (value === 0) continue;
    const [signal, band, aspect] = key.split('|');
    const [module, port] = (ASPECT_KNOB as Record<string, [string, string]>)[aspect];
    const from =
      signal === 'loudness' && band === '-'
        ? { module: 'envelope', port: 'level' }
        : signal === 'track'
          ? { module: 'track', port: 'pulse' }
          : { module: 'music', port: signal, reference: 'absolute' as const, ...(band === '-' ? {} : { channel: Number(band) }) };
    cables.push({ id: key, from, to: { module, port }, value: value as CableV2['value'], mode: (LEVEL_ASPECTS as readonly string[]).includes(aspect) ? 'level' : 'swing', ...(signal === 'loudness' && band === '-' ? { depth: null } : {}) });
  }
  const modules = ['music', 'track', 'envelope', 'clock'].map((id) => ({ id, type: id, knobs: {} }));
  return { v: 2, modules, cables, macros: [], pins: [] };
}

/** The one engine over the v2 patch's cables. */
const graph = (): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s.routes) {
        from = s.routes;
        p = v2Of(s.routes);
      }
      return stepEngine(st, p!, {}, m, () => 0, s, dt, opts).aspects;
    },
  };
};

test('the one engine over the cables of a v2 patch gives what the routes gave', async () => {
  const { CHAINS, chainRoutes } = await import('../src/lib/visuals/chains.ts');
  const variants: [string, typeof settings, boolean][] = [['the Cratarr chain', settings, false], ['reduced motion', settings, true]];
  for (const c of CHAINS) if (!c.extra.cables.length && !c.extra.modulators.length) variants.push([c.id, { ...settings, ...(c.settings ?? {}), routes: chainRoutes(c) } as typeof settings, false]);
  let worst = 0;
  for (const id of TRACKS) {
    const t = track(id);
    for (const [name, s, still] of variants) {
      const a = record(todays(), t, s, 30, 15, still);
      const b = record(graph(), t, s, 30, 15, still);
      for (const [k, d] of compare(a, b)) {
        worst = Math.max(worst, d.most);
        assert.ok(d.most <= 1e-4, `${id} ${name}: ${k} differs by ${d.most} at frame ${d.frame}`);
      }
    }
  }
});
