// The visuals' continuity list (0.10, the architecture audit's visuals step 5; the owner, 2026-10-29: continuous
// over discrete): what made a picture stand still, jump or go black where it should glide.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { track, TRACKS } from './harness/recorded.ts';

const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
import type { PatchV2 } from '../src/lib/visuals/ports.ts';

const types = v2TypeRecord();
const t = track(TRACKS[0]);

/** `p` stepped `frames` frames from 30 s into the groove: each frame's outcome. */
function run(p: PatchV2, frames: number, each?: (k: number, p: PatchV2) => void) {
  const st = newEngine();
  const music = newMusicState();
  const out = [];
  for (let k = 0; k < frames; k++) {
    each?.(k, p);
    const at = 30 + k / 30;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000 });
    out.push(structuredClone(stepEngine(st, p, types, m, () => 0, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL })));
  }
  return out;
}

test("a macro on a Grid's Count and Spacing lays it out anew", () => {
  const p: PatchV2 = {
    v: 2,
    modules: [{ id: 'grid', type: 'grid', knobs: { count: 4, spacing: 0.3 } }],
    cables: [],
    macros: [
      { id: 'more', on: 'rack', label: 'More', value: 0, targets: [{ module: 'grid', knob: 'count', from: 4, to: 16 }] },
      { id: 'wide', on: 'rack', label: 'Wide', value: 0, targets: [{ module: 'grid', knob: 'spacing', from: 0.1, to: 0.5 }] },
    ],
    pins: [],
  } as unknown as PatchV2;
  const at = (more: number, wide: number) => {
    p.macros[0].value = more;
    p.macros[1].value = wide;
    // Twice: the Grid reads its macros as the frame before left them.
    return run(p, 2).at(-1)!.points.grid;
  };
  assert.equal(at(0, 0).length, 4);
  assert.equal(at(1, 0).length, 16, 'More all the way: sixteen');
  const spread = (ps: { x: number; y: number }[]) => Math.max(...ps.map((q) => Math.hypot(q.x, q.y)));
  assert.ok(spread(at(1, 1)) > spread(at(1, 0)) * 2, 'Wide spreads them');
});

test('the Flame follows a choice a macro turns frame after frame, no longer standing still until it stops', async () => {
  const { DEFAULTS } = await import('../src/lib/visuals/settings.svelte.ts');
  const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
  const p = flatten(toV2(structuredClone(DEFAULTS)));
  // The look's own macros off: the Form knob is turned here, as a macro gliding over it turns it.
  p.macros = p.macros.filter((x) => !x.id.startsWith('look-'));
  const flame = p.modules.find((x) => x.type === 'flame')!;
  const st = newEngine();
  const music = newMusicState();
  const numbers = (x: unknown, out: number[] = []): number[] => {
    if (typeof x === 'number') out.push(x);
    else if (x && typeof x === 'object') for (const v of Object.values(x)) numbers(v, out);
    return out;
  };
  const apart: number[] = [];
  for (let k = 0; k < 40; k++) {
    // Form turning a twentieth a frame from the second third of a second on.
    if (k >= 10) flame.knobs.form = (flame.knobs.form ?? 0) + 0.05;
    const at = 20 + k / 30;
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting });
    stepEngine(st, p, types, m, () => 0, 1 / 30, { still: false, now: at * 1000, fluidFull: FLUID_FULL, aspect: 16 / 9, view: { cover: { key: null, colours: null }, glide: null } });
    const n = st.flameNodes[flame.id];
    const [a, b] = [numbers(n.shown), numbers(n.to)];
    apart.push(a.length === b.length ? a.reduce((s, v, i) => s + Math.abs(v - b[i]), 0) : NaN);
  }
  // How far what shows is from where Form is going: closing while Form moves (before, each frame's blend began anew
  // from what showed, and it stood where it was until Form stopped).
  assert.ok(apart[22] > 0, 'Form has moved the look');
  assert.ok(apart[38] < 0.8 * apart[22], `what shows ${apart[22].toFixed(2)} from the look at frame 22, ${apart[38].toFixed(2)} at 38`);
});

test('a Lens on a picture is drawn: each of its kinds a warp the pass bends by, or a shape it lets the picture through', async () => {
  const { PICTURE_NODES, passShaders } = await import('../src/lib/visuals/nodes/modules.ts');
  const { LENSES, lensWarpOf, VARIATIONS } = await import('../src/lib/visuals/flame.ts');
  const { WARPS } = await import('../src/lib/visuals/warps.ts');
  // Before: no pass, so the compositor made no node and the Screen got nothing.
  assert.ok(PICTURE_NODES.has('lens'));
  const glsl = passShaders('lens')![1];
  const warps = new Set<number>(Object.values(WARPS));
  for (let kind = 1; kind <= LENSES.length; kind++) {
    const v = lensWarpOf(kind);
    if (warps.has(v)) assert.ok(glsl.includes(`v == ${v})`), `${LENSES[kind - 1]}: bent by the pass`);
    else assert.ok(v >= VARIATIONS.cratarr, `${LENSES[kind - 1]}: a shape (${v}), the picture let through`);
  }
});

test('a choice glides: the compositor draws the one before and the new one and crossfades them over its time', async () => {
  const { choiceGlide, CHOICE_GLIDE_MS } = await import('../src/lib/visuals/nodes/walker.ts');
  const steps = ['kind'];
  let g = choiceGlide(undefined, steps, { kind: 2, amount: 0.5 }, 0);
  assert.equal(g.from, null, 'the first frame: nothing to glide from');
  g = choiceGlide(g.glide, steps, { kind: 2, amount: 0.6 }, 16);
  assert.equal(g.from, null, 'a continuous knob moving: no glide (it moves by itself)');
  g = choiceGlide(g.glide, steps, { kind: 3, amount: 0.6 }, 100);
  assert.deepEqual(g.from, { kind: 2, amount: 0.6 }, 'the choice before, the rest as it is now');
  assert.equal(g.s, 0, 'all of the one before at first');
  const half = choiceGlide(g.glide, steps, { kind: 3, amount: 0.7 }, 100 + CHOICE_GLIDE_MS / 2);
  assert.deepEqual(half.from, { kind: 2, amount: 0.7 });
  assert.ok(Math.abs(half.s - 0.5) < 1e-9, 'halfway through its time, half and half (eased)');
  assert.equal(choiceGlide(half.glide, steps, { kind: 3, amount: 0.7 }, 100 + CHOICE_GLIDE_MS).from, null, 'then the new one alone');
  // Changed again while gliding: from the choice just before, anew.
  let h = choiceGlide(undefined, steps, { kind: 1 }, 0);
  h = choiceGlide(h.glide, steps, { kind: 2 }, 10);
  h = choiceGlide(h.glide, steps, { kind: 3 }, 20);
  assert.deepEqual(h.from, { kind: 2 });
  assert.equal(h.s, 0);
});
