// What can be done to cables (cable-actions.ts, decision 0056): routes and extras alike, removed, scaled, walking,
// muted, made new.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cableOf, reroute, freeCableId, isRouteId, newCable, removeCables, type Cables } from '../src/lib/visuals/cable-actions.ts';
import { emptyExtra, type Cable } from '../src/lib/visuals/patch.ts';
import { routeKey } from '../src/lib/visuals/routes.ts';

const R = routeKey('beat', null, 'size');
const R2 = routeKey('level', 2, 'hue');
const X: Cable = { id: 'cable-1', from: { modulator: 'lfo' }, to: { module: 'flame', module2: undefined } as never, value: 0.4, mode: 'swing' };
const cables = (): Cables => ({ routes: { [R]: 0.5, [R2]: 1 }, extra: { ...emptyExtra(), cables: [{ ...X, to: { module: 'flame', knob: 'detail' } }] } });

test('route ids and extra ids told apart', () => {
  assert.equal(isRouteId(R), true);
  assert.equal(isRouteId('cable-1'), false);
});

test('removed: a route to 0 (so its default does not come back), an extra dropped', () => {
  const c = removeCables(cables(), [R, 'cable-1']);
  assert.equal(c.routes[R], 0);
  assert.equal(c.routes[R2], 1);
  assert.equal(c.extra.cables.length, 0);
});

test("new: a route where it can be one, else an extra; a level knob's cable lets through", () => {
  assert.equal(newCable({ signal: 'beat' }, { module: 'flame', knob: 'zoom' }, 'level', 'cable-9').id, R);
  const ref = newCable({ signal: 'beat', reference: 'lately' }, { module: 'flame', knob: 'zoom' }, 'swing', 'cable-9');
  assert.equal(ref.id, 'cable-9');
  const mod = newCable({ modulator: 'lfo' }, { module: 'flame', knob: 'detail' }, 'level', 'cable-9');
  assert.deepEqual([mod.id, mod.mode, mod.value], ['cable-9', 'level', 0.5]);
  assert.equal(freeCableId(cables()), 'cable-2');
});

test('a route read as a cable; rerouted: its value kept, a route where it can be, else an extra', () => {
  const c = cables();
  assert.deepEqual(cableOf(c, R), { id: R, from: { signal: 'beat' }, to: { module: 'flame', knob: 'zoom' }, value: 0.5, mode: 'swing' });
  assert.equal(cableOf(c, routeKey('beat', null, 'fluid'))?.mode ?? 'level', 'level');
  // Beat → Size moved to Hue: still a route.
  const hue = reroute(c, R, 'to', { module: 'flame', knob: 'hue' }, 'swing')!;
  assert.equal(hue.id, routeKey('beat', null, 'hue'));
  assert.equal(hue.cables.routes[hue.id], 0.5);
  assert.equal(hue.cables.routes[R], 0);
  // Moved onto a knob no column reaches: an extra now.
  const detail = reroute(c, R, 'to', { module: 'flame', knob: 'detail' }, 'swing')!;
  assert.equal(detail.id, 'cable-2');
  assert.deepEqual(detail.cables.extra.cables.at(-1)?.to, { module: 'flame', knob: 'detail' });
  // An extra's source changed: its id kept.
  const m = reroute(c, 'cable-1', 'from', { signal: 'swell' }, 'swing')!;
  assert.equal(m.id, 'cable-1');
  assert.deepEqual(m.cables.extra.cables[0].from, { signal: 'swell' });
  assert.equal(reroute(c, 'nope', 'to', { module: 'flame', knob: 'hue' }, 'swing'), null);
});
