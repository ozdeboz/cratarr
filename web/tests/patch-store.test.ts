// The patch's store (patch-store.ts, decision 0057 §7): every change undoable, a drag one step, redo cleared by a new
// change: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canRedo, canUndo, change, DEPTH, newStore, redo, replace, undo } from '../src/lib/visuals/patch-store.ts';

const patch = () => ({ v: 2 as const, modules: [{ id: 'lfo', type: 'lfo', knobs: { rate: 0.25 } }], cables: [], macros: [], pins: [] });
const setRate = (v: number) => (p: ReturnType<typeof patch>) => void (p.modules[0].knobs.rate = v);

test('a change, undone and redone', () => {
  let s = newStore(patch());
  s = change(s, setRate(1), 'knob:lfo.rate', 0);
  assert.equal(s.now.modules[0].knobs.rate, 1);
  s = undo(s);
  assert.equal(s.now.modules[0].knobs.rate, 0.25);
  assert.ok(canRedo(s) && !canUndo(s));
  s = redo(s);
  assert.equal(s.now.modules[0].knobs.rate, 1);
});

test('a drag is one step; a pause, or another knob, starts the next', () => {
  let s = newStore(patch());
  for (const [v, at] of [[0.3, 0], [0.4, 100], [0.5, 200]] as const) s = change(s, setRate(v), 'knob:lfo.rate', at);
  assert.equal(s.past.length, 1);
  s = change(s, setRate(0.9), 'knob:lfo.rate', 5000);
  assert.equal(s.past.length, 2);
  assert.equal(undo(undo(s)).now.modules[0].knobs.rate, 0.25);
});

test('no step for no change; a new change clears redo; history kept to its depth', () => {
  let s = newStore(patch());
  s = change(s, setRate(0.25));
  assert.ok(!canUndo(s));
  s = undo(change(s, setRate(1)));
  s = change(s, setRate(2));
  assert.ok(!canRedo(s));
  for (let i = 0; i < DEPTH + 20; i++) s = change(s, setRate(i + 10));
  assert.equal(s.past.length, DEPTH);
});

test('a patch from elsewhere is one step back', () => {
  let s = newStore(patch());
  const other = { ...patch(), modules: [{ id: 'drift', type: 'drift', knobs: {} }] };
  s = replace(s, other);
  assert.equal(s.now.modules[0].id, 'drift');
  assert.equal(undo(s).now.modules[0].id, 'lfo');
});
