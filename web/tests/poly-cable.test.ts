// A polyphonic cable (VCV's; decision 0057 §4b): a cable from a band's signal with no channel of its own carries all
// six, each to its band, so one cable does what six copies, one a band, did. The engine over a patch with its band
// copies merged gives what it gave over the copies, frame by frame.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, compare, record, track, type Engine } from './harness/recorded.ts';

const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { routeKey } = await import('../src/lib/visuals/routes.ts');
type PatchV2 = import('../src/lib/visuals/ports.ts').PatchV2;
type CableV2 = import('../src/lib/visuals/ports.ts').CableV2;

/** Each set of six band copies (the same source port and reading, target, value, mode and depth) as one cable. */
function merged(p: PatchV2): { patch: PatchV2; merges: number } {
  const groups = new Map<string, CableV2[]>();
  const rest: CableV2[] = [];
  for (const c of p.cables) {
    if (c.from.channel === undefined) {
      rest.push(c);
      continue;
    }
    const { channel: _, ...from } = c.from;
    const key = JSON.stringify([from, c.to, c.value, c.mode, c.depth]);
    (groups.get(key) ?? groups.set(key, []).get(key)!).push(c);
  }
  let merges = 0;
  for (const cs of groups.values()) {
    if (new Set(cs.map((c) => c.from.channel)).size !== 6) {
      rest.push(...cs);
      continue;
    }
    const { channel: _, ...from } = cs[0].from;
    rest.push({ ...cs[0], id: `${cs[0].id}-all`, from });
    merges++;
  }
  return { patch: { ...p, cables: rest }, merges };
}

const over = (convert: (p: PatchV2) => PatchV2): Engine => {
  const st = newEngine();
  let p: PatchV2 | null = null;
  let from: unknown = null;
  return {
    name: 'engine',
    step: (m, s, dt, opts) => {
      if (from !== s) {
        from = s;
        p = convert(flatten(toV2(s, { follow: false })));
      }
      return stepEngine(st, p!, {}, m, () => 0, dt, opts);
    },
  };
};

/** Every band into a part's column (each part its band's) and into another (their mean), swings and levels. */
const all = structuredClone(DEFAULTS);
all.routes = {
  ...all.routes,
  ...Object.fromEntries(
    [0, 1, 2, 3, 4, 5].flatMap((b) => [
      [routeKey('level', b, 'rotation'), 0.9],
      [routeKey('flux', b, 'brightness'), 0.6],
      [routeKey('level', b, 'tunnel'), 0.5],
    ]),
  ),
};

test('six band copies merged into one cable: the same picture, frame by frame, on every recorded track', () => {
  const { merges } = merged(flatten(toV2(all, { follow: false })));
  assert.ok(merges >= 3, `${merges} merges`);
  for (const id of TRACKS) {
    const t = track(id);
    const a = record(over((p) => p), t, all, 30, 20, false);
    const b = record(over((p) => merged(p).patch), t, all, 30, 20, false);
    for (const [k, d] of compare(a, b)) assert.ok(d.most <= 1e-9, `${id}: ${k} differs by ${d.most} at frame ${d.frame}`);
  }
});
