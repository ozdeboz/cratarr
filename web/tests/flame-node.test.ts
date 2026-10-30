// The Flame's own look, framing and place on the screen (engine/flame-node.ts; the owner, 2026-10-29: "engine
// purity"), worked out by the engine where it runs with a view, no longer by the page's frame loop: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { TRACKS, track } from './harness/recorded.ts';
const { DEFAULTS, FLUID_FULL } = await import('../src/lib/visuals/settings.svelte.ts');
const { newEngine, stepEngine } = await import('../src/lib/visuals/engine/graph.ts');
const { flatten, toV2 } = await import('../src/lib/visuals/v2.ts');
const { musicOf, newMusicState } = await import('../src/lib/visuals/music-core.ts');
const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { LOOK_MS } = await import('../src/lib/visuals/engine/flame-node.ts');

const types = v2TypeRecord();
const NO_COVER = { key: null, colours: null };

/** The default rack run `frames` frames from `at` seconds, with a view; `change` turns its Flame's knobs at a frame. */
function run(frames: number, o: { change?: { frame: number; knobs: Record<string, number>; glide?: number; macros?: Record<string, number> }; fluid?: number; macros?: boolean } = {}) {
  const t = track(TRACKS[0]);
  const music = newMusicState();
  const p = flatten(toV2(structuredClone(DEFAULTS)));
  const flame = p.modules.find((x) => x.type === 'flame')!;
  // The Flame's knobs by hand: its look's macros (0077) off, else they set them.
  if (!o.macros) p.macros = p.macros.filter((x) => !x.id.startsWith('look-'));
  if (o.fluid !== undefined) p.modules.find((x) => x.type === 'fluid')!.knobs.fluid = o.fluid;
  const st = newEngine();
  const out = [];
  for (let k = 0; k < frames; k++) {
    const at = 20 + k / 30;
    let glide: number | null = null;
    if (o.change && k === o.change.frame) {
      Object.assign(flame.knobs, o.change.knobs);
      for (const [id, v] of Object.entries(o.change.macros ?? {})) p.macros.find((x) => x.id === id)!.value = v;
      glide = o.change.glide ?? null;
    }
    const m = musicOf(t.signals, t.beats, at, true, { state: music, at: at * 1000, lighting: DEFAULTS.lighting });
    out.push(stepEngine(st, p, types, m, () => 0, 1 / 30, { now: at * 1000, fluidFull: FLUID_FULL, aspect: 16 / 9, view: { cover: NO_COVER, glide } }));
  }
  return { out, flame, st };
}

test('the Flame draws from its own knobs with a view, and none without one', () => {
  const { out, flame } = run(2);
  const v = out[1].flames[flame.id];
  assert.ok(v && v.drawn.xforms.length > 0 && v.camera.xforms.length === v.drawn.xforms.length);
  assert.equal(v.mode, 'rainbow');
  const t = track(TRACKS[0]);
  const m = musicOf(t.signals, t.beats, 20, true, { state: newMusicState(), at: 20000, lighting: DEFAULTS.lighting });
  const blind = stepEngine(newEngine(), flatten(toV2(structuredClone(DEFAULTS))), types, m, () => 0, 1 / 30, { now: 20000, fluidFull: FLUID_FULL });
  assert.deepEqual(blind.flames, {});
});

test('a look changed by degrees follows at once; a new form glides over its time, or the cause’s', () => {
  const hue = run(4, { change: { frame: 2, knobs: { hue: 0.4 } } });
  const before = hue.out[1].flames[hue.flame.id].drawn;
  const after = hue.out[2].flames[hue.flame.id].drawn;
  assert.notDeepEqual(after.xforms.map((x) => x.color), before.xforms.map((x) => x.color));
  // The new form, half way through its glide: neither the old one nor the new.
  const half = Math.round(LOOK_MS / 2 / (1000 / 30));
  const form = run(2 + half + LOOK_MS / 30 + 4, { change: { frame: 2, knobs: { form: 7 } } });
  const id = form.flame.id;
  const old = form.out[1].flames[id].drawn.xforms[0].affine;
  const mid = form.out[2 + half].flames[id].drawn.xforms[0].affine;
  const end = form.out[form.out.length - 1].flames[id].drawn.xforms[0].affine;
  assert.notDeepEqual(mid, old);
  assert.notDeepEqual(mid, end);
  // A cause's time (a new track's look: 6 s) is still gliding where a knob's would be done.
  const slow = run(2 + LOOK_MS / 30 + 6, { change: { frame: 2, knobs: { form: 7 }, glide: 6000 } });
  const quick = run(2 + LOOK_MS / 30 + 6, { change: { frame: 2, knobs: { form: 7 } } });
  const last = (r: typeof slow) => r.out[r.out.length - 1].flames[r.flame.id].drawn.xforms[0].affine;
  assert.notDeepEqual(last(slow), last(quick));
});

test('framed as far as its Fit, measured over frames; its outputs and its parts on the screen its own', () => {
  const { out, flame, st } = run(60);
  const last = out[out.length - 1];
  const v = last.flames[flame.id];
  // Fit 1 (the default): the camera moves to where its points fall.
  assert.ok(v.camera.center[0] !== v.drawn.center[0] || v.camera.center[1] !== v.drawn.center[1] || v.camera.size !== v.drawn.size);
  assert.ok((st.flameNodes[flame.id].frame?.size ?? 0) > 0, 'Size measured');
  assert.ok(v.parts.length > 0 && v.parts.some((q) => q && q[0] >= 0 && q[0] <= 1));
  const fluid = Object.keys(last.pointsAt).find((k) => k.endsWith('.at'))!;
  assert.deepEqual(last.pointsAt[fluid], v.parts);
});

test("the Fluid stirred by the engine: its pushes and flow, from its own Stir, Flow and Swirl", () => {
  const { out } = run(90, { fluid: 0.8 });
  const last = out[out.length - 1];
  assert.ok(last.fluid && Array.isArray(last.fluid.forces) && Number.isFinite(last.fluid.flow.energy));
  assert.ok(out.some((f) => (f.fluid?.forces.length ?? 0) > 0), 'kicked at least once over three seconds of a track');
});

test('a module taken out of the rack does nothing, whatever the settings from before the switch said', () => {
  const t = track(TRACKS[0]);
  const m = musicOf(t.signals, t.beats, 20, true, { state: newMusicState(), at: 20000, lighting: DEFAULTS.lighting });
  const p = flatten(toV2(structuredClone(DEFAULTS)));
  p.modules = p.modules.filter((x) => x.type !== 'fluid' && x.type !== 'tunnel' && x.type !== 'dive');
  p.cables = p.cables.filter((c) => p.modules.some((x) => x.id === c.from.module) && p.modules.some((x) => x.id === c.to.module));
  const old = { ...structuredClone(DEFAULTS), fluid: 0.8, tunnel: 0.6, dive: 0.5 };
  const f = stepEngine(newEngine(), p, types, m, () => 0, 1 / 30, { now: 20000, fluidFull: FLUID_FULL });
  assert.equal(f.device!.meter.fluid, 0);
  assert.equal(f.device!.meter.tunnel, 0);
  assert.equal(f.device!.meter.dive, 0);
});

test("the look's macros move the Flame's look: Hue by degrees at once, as its knob would", () => {
  const r = run(4, { macros: true, change: { frame: 2, knobs: {}, macros: { 'look-hue': 0.9 } } });
  const before = r.out[1].flames[r.flame.id].drawn.xforms.map((x) => x.color);
  const after = r.out[2].flames[r.flame.id].drawn.xforms.map((x) => x.color);
  assert.notDeepEqual(after, before);
});
