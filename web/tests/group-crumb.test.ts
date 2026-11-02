// A panel's group as a level of its breadcrumb (lib/groupCrumb.ts; the owner, 2026-11-02): a group's pick lands on the
// part last seen in it, else its top one; what is remembered is per panel and group.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { groupsOf, landingOf, remembered } = await import('../src/lib/groupCrumb.ts');

const parts = [
  { id: 'account', group: 'personal' },
  { id: 'appearance', group: 'device' },
  { id: 'layout', group: 'device' },
  { id: 'player', group: 'device' },
  { id: 'libraries', group: 'server' },
];

test('the groups in their order, each once', () => {
  assert.deepEqual(groupsOf(parts), ['personal', 'device', 'server']);
  assert.deepEqual(groupsOf([]), []);
});

test("a group's pick lands on its top part where none was seen", () => {
  assert.equal(landingOf(parts, 'device'), 'appearance');
  assert.equal(landingOf(parts, 'device', null), 'appearance');
});

test('on the part last seen in it', () => {
  assert.equal(landingOf(parts, 'device', 'player'), 'player');
});

test('on its top part where the one seen is gone, or is not one of its own', () => {
  assert.equal(landingOf(parts, 'device', 'tracking'), 'appearance');
  assert.equal(landingOf(parts, 'device', 'libraries'), 'appearance');
});

test('nowhere for a group with no parts', () => {
  assert.equal(landingOf(parts, 'activity'), null);
});

test('remembered per panel and group, the same object where nothing changes', () => {
  const a = remembered({}, 'settings', 'device', 'layout');
  assert.deepEqual(a, { 'settings/device': 'layout' });
  assert.equal(remembered(a, 'settings', 'device', 'layout'), a);
  const b = remembered(a, 'health', 'device', 'x');
  assert.deepEqual(b, { 'settings/device': 'layout', 'health/device': 'x' });
  assert.deepEqual(remembered(b, 'settings', 'device', 'player')['settings/device'], 'player');
});
