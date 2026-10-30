// The aspect dice (the owner, 2026-10-30: "all nodes should select by what they mean"): every knob of every module type
// declares what it means (engine/knob-aspects `withAspects`, where its module declares its knobs), so none falls back to
// the old matching or out of Spatial, Motion, Colours or Lightness by a rename; Spatial reaches a source's spatial
// knobs, Motion its speeds; and a knob of the same name means what its own module says.
import { test } from 'node:test';
import assert from 'node:assert/strict';
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;

const { v2TypeRecord } = await import('../src/lib/visuals/v2-types.ts');
const { aspectOf } = await import('../src/lib/visuals/engine/knob-aspects.ts');
const { scopeNamed } = await import('../src/lib/visuals/engine/randomize.ts');

const types = v2TypeRecord();
const knobOf = (type: string, id: string) => {
  const k = types[type]?.knobs.find((x) => x.id === id);
  assert.ok(k, `${type}.${id} exists`);
  return k;
};
const moduleOf = (type: string) => ({ id: type, type, knobs: {} });

test('every knob of every module type declares its aspect (none falls back to the old matching)', () => {
  const undeclared = Object.values(types).flatMap((t) => t.knobs.filter((k) => aspectOf(k) === undefined).map((k) => `${t.type}.${k.id}`));
  assert.deepEqual(undeclared, []);
});

test('Spatial reaches a source’s place and size, Motion its spin, neither its colour', () => {
  const spatial = scopeNamed('spatial', types)!;
  const motion = scopeNamed('motion', types)!;
  const colours = scopeNamed('colours', types)!;
  const m = moduleOf('waves');
  for (const id of ['x', 'y', 'radius', 'rotation']) assert.ok(spatial(m, knobOf('waves', id)), `spatial reaches waves.${id}`);
  assert.ok(motion(m, knobOf('waves', 'spin')), 'motion reaches waves.spin');
  assert.ok(!spatial(m, knobOf('waves', 'hue')) && !motion(m, knobOf('waves', 'hue')), 'hue is neither');
  assert.ok(colours(m, knobOf('waves', 'hue')), 'colours reaches waves.hue');
});

test('a knob means what its own module says, not what its name means elsewhere', () => {
  const spatial = scopeNamed('spatial', types)!;
  const motion = scopeNamed('motion', types)!;
  const lightness = scopeNamed('lightness', types)!;
  // One table by knob id gave these the meaning of a namesake: a roll on Spatial moved a Dice's own reach.
  assert.ok(!spatial(moduleOf('dice'), knobOf('dice', 'reach')), 'a Dice’s Reach is not spatial');
  assert.ok(!motion(moduleOf('dice'), knobOf('dice', 'glide')), 'a Dice’s Glide is not motion');
  assert.ok(!spatial(moduleOf('sequencer'), knobOf('sequencer', 'length')), 'a Sequencer’s Length is not spatial');
  assert.ok(!spatial(moduleOf('shaper'), knobOf('shaper', 'h1x')), 'a Shaper’s handle is not spatial');
  assert.ok(!lightness(moduleOf('envelope'), knobOf('envelope', 'intensity')), 'an Envelope’s Intensity is not lightness');
  // And the same names where they do mean it.
  assert.ok(spatial(moduleOf('fold'), knobOf('fold', 'reach')), 'a Fold’s Reach is spatial');
  assert.ok(spatial(moduleOf('grid'), knobOf('grid', 'spacing')), 'a Grid’s Spacing is spatial');
  assert.ok(motion(moduleOf('random'), knobOf('random', 'rate')), 'a Random’s Rate is motion');
  assert.ok(motion(moduleOf('clock'), knobOf('clock', 'speed')), 'the Clock’s Speed is motion');
  assert.ok(lightness(moduleOf('fold'), knobOf('fold', 'lift')), 'a Fold’s Lift is lightness');
  // The Flame's Colours (its hue family) is a colour, as its label always made it.
  assert.ok(scopeNamed('colours', types)!(moduleOf('flame'), knobOf('flame', 'family')), 'the Flame’s Colours is a colour');
});

test('an effect’s knobs carry its effect’s declared aspects', () => {
  assert.equal(aspectOf(knobOf('fx-kaleid', 'sides')), 'spatial');
  assert.equal(aspectOf(knobOf('fx-kaleid', 'spin')), 'motion');
  assert.equal(aspectOf(knobOf('fx-kaleid', 'amount')), null);
});
