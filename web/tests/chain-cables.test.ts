// Cables from the patch's extras into today's chain (chain-cables.ts, decision 0056): given as routes into the same
// columns are, beside them.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cableX, columnOf, giveCables, type CableSources } from '../src/lib/visuals/chain-cables.ts';
import { A, compile, give, given, T, TRACK_SIGNALS, BAND_SIGNALS, routeKey } from '../src/lib/visuals/routes.ts';
import type { Cable } from '../src/lib/visuals/patch.ts';

const BANDS = 6;
const near = (a: number, b: number, eps = 1e-6) => assert.ok(Math.abs(a - b) <= eps, `${a} is not near ${b}`);
const cable = (id: string, from: Cable['from'], module: string, knob: string, value: number, mode: Cable['mode'] = 'swing'): Cable => ({ id, from, to: { module, knob }, value, mode });
const sources = (sig: Record<string, number> = {}, outputs: Record<string, number> = {}, modulators: CableSources['modulators'] = []): CableSources => ({
  signals: (ref) => sig[`${ref.signal}${ref.band ?? ''}`] ?? 0,
  outputs,
  modulators,
});

test("a cable reaches its knob's column; a knob that is no column, none", () => {
  assert.equal(columnOf(cable('a', { signal: 'beat' }, 'flame', 'hue', 1)), A.hue);
  assert.equal(columnOf(cable('b', { signal: 'beat' }, 'show', 'glow', 1)), A.brightness);
  assert.equal(columnOf(cable('c', { signal: 'beat' }, 'flame', 'detail', 1)), null);
  assert.equal(columnOf(cable('d', { signal: 'beat' }, 'shapes-1', 'size', 1)), null);
});

test('x: a signal at 1 = typical is half way; a modulator as it is, a bipolar one from its middle', () => {
  const src = sources({ beat: 1, level2: 9 }, { lfo: -1, fol: 0.3 }, [
    { id: 'lfo', type: 'lfo', knobs: {} },
    { id: 'fol', type: 'fast', knobs: {} },
  ]);
  near(cableX(cable('a', { signal: 'beat' }, 'flame', 'hue', 1), src)!, 0.5);
  near(cableX(cable('a', { signal: 'level', band: 2 }, 'flame', 'hue', 1), src)!, 1);
  near(cableX(cable('a', { modulator: 'fol' }, 'flame', 'hue', 1), src)!, 0.3);
  assert.equal(cableX(cable('a', { modulator: 'gone' }, 'flame', 'hue', 1), src), null);
});

test('a swing adds to its column as a route does; a level multiplies; the loudness is not enveloped', () => {
  // The same as a route of 0.6 from Beat into Rotation: given alike.
  const route = given(BANDS);
  const track = new Float32Array(TRACK_SIGNALS.length);
  track[T.beat] = 0.5;
  give(compile({ [routeKey('beat', null, 'rotation')]: 0.6 }, BANDS), track, Array.from({ length: BANDS }, () => new Float32Array(BAND_SIGNALS.length)), 0.8, route);
  const out = given(BANDS);
  giveCables(out, [cable('c', { signal: 'beat' }, 'flame', 'rotation', 0.6)], sources({ beat: 1 }), 0.8);
  near(out.whole[A.rotation], route.whole[A.rotation]);
  // A level (Fluid's) at its peak lets all through, less below it.
  const lv = given(BANDS);
  giveCables(lv, [cable('l', { signal: 'loudness' }, 'fluid', 'fluid', 1, 'level')], sources({ loudness: 1 }), 0);
  near(lv.level[A.fluid], 0.5, 1e-6);
});

test("a band's cable: its part's own column, else the bands' mean (a moment's: their largest)", () => {
  const out = given(BANDS);
  const src = sources({ level3: 2 });
  giveCables(out, [cable('h', { signal: 'level', band: 3 }, 'flame', 'hue', 1), cable('s', { signal: 'level', band: 3 }, 'show', 'size', 1), cable('m', { signal: 'level', band: 3 }, 'flame', 'symmetry', 1)], src, 1);
  near(out.band[3][A.hue], 1);
  near(out.whole[A.hue], 0);
  near(out.whole[A.size], 1 / BANDS);
  near(out.whole[A.symmetry], 1);
});

test('a walking cable gives its walk factor of it', () => {
  const out = given(BANDS);
  const walking = { h1: [1 / 3, 1 / 6], h2: [2 / 3, 1 / 3], h: 0.5, walks: true } as unknown as number;
  giveCables(out, [cable('w', { signal: 'beat' }, 'flame', 'rotation', walking)], sources({ beat: 2 }), 1, { w: 0.25 });
  near(out.whole[A.rotation], 0.25 * 0.5, 1e-3);
});
