// The rules crate's index in the page (lib/indexEngine on the real index.wasm, the crate's own files): bytes in and out over its
// memory, refusals by name, and the rule on what to fetch as the rules crate's own tests have it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const { instantiateIndex } = await import('../../src/lib/indexEngine.ts');
type Part = import('../../src/lib/indexEngine.ts').Part;

const vector = (name: string) => new Uint8Array(readFileSync(new URL(`../../../server/rules/tests/vectors/index/${name}`, import.meta.url)));
const module = await WebAssembly.compile(readFileSync(new URL('../../src/lib/wasm/index.wasm', import.meta.url)));
const engine = () => instantiateIndex(module);
const text = (name: string) => JSON.parse(new TextDecoder().decode(vector(name)));

const whole: Part = { whole: true, stopped: false, lost: 0, failed: 0, tried: null, started: 0 };
const held: Part = { ...whole, whole: false };
const LATER = 20_000;

test('bytes in and bytes out: what is loaded saves as it came, at its rev', async () => {
  const e = await engine();
  assert.equal(e.loadIndex(vector('whole.bin')), null);
  assert.equal(e.indexRev(), 5);
  assert.equal(e.tracks(), 5);
  assert.deepEqual(e.saveIndex(), vector('whole.bin'));
});

test('the feed applied is the index after it, to the byte', async () => {
  const e = await engine();
  e.loadIndex(vector('whole.bin'));
  assert.equal(e.applyIndex(vector('changes.bin')), null);
  assert.equal(e.indexRev(), 9);
  assert.deepEqual(e.saveIndex(), vector('after.bin'));
});

test('the sounds go the same way, on their own', async () => {
  const e = await engine();
  assert.equal(e.loadSounds(vector('sounds.bin')), null);
  assert.equal(e.soundsRev(), 5);
  assert.equal(e.applySounds(vector('sound-changes.bin')), null);
  assert.equal(e.soundsRev(), 9);
  assert.deepEqual(e.saveSounds(), vector('sounds-after.bin'));
  assert.equal(e.indexRev(), -1, 'and hold no index');
});

test('the rows read as the rules crate wrote them', async () => {
  const e = await engine();
  e.loadIndex(vector('whole.bin'));
  assert.deepEqual(e.list(), text('list.json'));
  assert.deepEqual(e.record(10), text('record.json'));
  assert.equal(e.record(99), null);
});

test('with nothing held, nothing is there to say', async () => {
  const e = await engine();
  assert.equal(e.saveIndex(), null);
  assert.equal(e.saveSounds(), null);
  assert.equal(e.list(), null);
  assert.equal(e.indexRev(), -1);
  assert.deepEqual(e.applyIndex(vector('changes.bin')), { why: 'rev' });
});

test('another version is refused by name, with the two numbers', async () => {
  const e = await engine();
  const bytes = vector('whole.bin');
  bytes[4] = 9;
  assert.deepEqual(e.loadIndex(bytes), { why: 'version', found: 9, wants: e.indexVersion() });
});

test('cut short, a changed byte, and what does not fit are each said', async () => {
  const e = await engine();
  assert.deepEqual(e.loadIndex(vector('whole.bin').slice(0, 100)), { why: 'damaged' });
  const changed = vector('whole.bin');
  changed[200] ^= 1;
  assert.deepEqual(e.loadIndex(changed), { why: 'damaged' });
  e.loadIndex(vector('after.bin'));
  assert.deepEqual(e.applyIndex(vector('changes.bin')), { why: 'rev' }, 'not where it stands');
  assert.equal(e.indexRev(), 9, 'and what was held stays');
});

test('going round many times leaves the module no bigger: what it gave and took is freed', async () => {
  const e = await engine();
  const bytes = vector('whole.bin');
  e.loadIndex(bytes);
  e.list();
  const memory = (e as unknown as { x: { memory: WebAssembly.Memory } }).x.memory;
  const before = memory.buffer.byteLength;
  for (let i = 0; i < 3000; i++) {
    e.loadIndex(bytes);
    e.list();
    e.saveIndex();
  }
  assert.equal(memory.buffer.byteLength, before);
});

test('the rule on what to fetch reads as the crate has it', async () => {
  const e = await engine();
  assert.deepEqual(e.step(whole, 'free', 'unasked', false, 0), { do: 'wait', until: e.firstAfter() });
  assert.deepEqual(e.step(whole, 'free', 'unasked', false, LATER), { do: 'whole' });
  assert.deepEqual(e.step(whole, 'metered', 'unasked', false, LATER), { do: 'ask' });
  assert.deepEqual(e.step(whole, 'metered', 'yes', false, LATER), { do: 'whole' });
  assert.deepEqual(e.step(whole, 'metered', 'no', false, LATER), { do: 'rest' });
  assert.deepEqual(e.step(whole, 'off', 'yes', false, LATER), { do: 'rest' });
  assert.deepEqual(e.step(held, 'metered', 'no', true, LATER), { do: 'changes' });
  assert.deepEqual(e.step(held, 'metered', 'no', false, LATER), { do: 'rest' });
  assert.deepEqual(e.step({ ...held, stopped: true }, 'free', 'yes', true, LATER), { do: 'stopped' });
});

test('how a part stands after each way a fetch can end', async () => {
  const e = await engine();
  assert.deepEqual(e.after({ ...whole, lost: 2, failed: 1 }, 'took', LATER), { ...held, tried: LATER });
  assert.deepEqual(e.after(held, 'lost', LATER), { ...held, lost: 1, tried: LATER });
  assert.deepEqual(e.after(held, 'failed', LATER), { ...held, failed: 1, tried: LATER });
  assert.deepEqual(e.after(held, 'start-again', LATER), { ...whole, tried: LATER });
  assert.deepEqual(e.after(whole, 'version', LATER), { ...whole, stopped: true, tried: LATER });
});

test('a start forgets what went wrong, and a stop stands', async () => {
  const e = await engine();
  assert.deepEqual(e.after({ ...held, lost: 2, failed: 3, tried: 5, stopped: true }, 'start', LATER), { ...held, stopped: true, started: LATER });
});

test('a start over what another version kept forgets what went wrong, lifts its stop, and comes whole', async () => {
  const e = await engine();
  assert.deepEqual(e.after({ ...held, lost: 2, failed: 3, tried: 5, stopped: true }, 'start-over-another', LATER), { ...whole, started: LATER });
});

test('how an answer ended a fetch is the crate\'s to say', async () => {
  const e = await engine();
  assert.equal(e.answer(null, false), 'lost');
  assert.equal(e.answer(401, true), 'lost');
  assert.equal(e.answer(502, false), 'lost');
  assert.equal(e.answer(200, true), 'bytes');
  assert.equal(e.answer(204, false), 'took');
  assert.equal(e.answer(204, true), 'failed');
  assert.equal(e.answer(410, false), 'start-again');
  assert.equal(e.answer(410, true), 'failed');
  assert.equal(e.answer(500, false), 'failed');
});

test('bytes fetched whole are held, each part its own, and what they came to is said', async () => {
  const e = await engine();
  assert.equal(e.takeWhole('index', vector('whole.bin')), 'took');
  assert.equal(e.indexRev(), 5);
  assert.equal(e.soundsRev(), -1, 'the sounds are not touched');
  assert.equal(e.takeWhole('sounds', vector('sounds.bin')), 'took');
  assert.equal(e.soundsRev(), 5);
  const other = vector('whole.bin').slice();
  other[4] = 9;
  assert.equal(e.takeWhole('index', other), 'version');
  assert.equal(e.takeWhole('index', vector('whole.bin').slice(0, 100)), 'failed');
});

test('a feed taken leaves the result held, each part its own, and one that cannot go on starts again', async () => {
  const e = await engine();
  assert.equal(e.takeChanges('index', vector('whole.bin'), vector('changes.bin')), 'took');
  assert.deepEqual(e.saveIndex(), vector('after.bin'));
  assert.equal(e.takeChanges('sounds', vector('sounds.bin'), vector('sound-changes.bin')), 'took');
  assert.deepEqual(e.saveSounds(), vector('sounds-after.bin'));
  assert.equal(e.takeChanges('index', vector('after.bin'), vector('changes.bin')), 'start-again');
  assert.equal(e.takeChanges('index', vector('whole.bin'), vector('changes.bin').slice(0, 50)), 'start-again');
  const other = vector('whole.bin').slice();
  other[4] = 9;
  assert.equal(e.takeChanges('index', other, vector('changes.bin')), 'start-again');
});

test('covers are taken by their head, each end as the crate says, and say where they stand', async () => {
  const e = await engine();
  const head = (version: number, rev: number) => new Uint8Array([...new TextEncoder().encode('CRXV'), ...new Uint8Array(new Uint32Array([version]).buffer), ...new Uint8Array(new BigInt64Array([BigInt(rev)]).buffer)]);
  assert.equal(e.coversRev(), -1);
  assert.equal(e.takeCovers(head(e.coversVersion(), 41), true), 'took');
  assert.equal(e.coversRev(), 41);
  assert.equal(e.takeCovers(head(e.coversVersion(), 42), false), 'took');
  assert.equal(e.coversRev(), 42);
  assert.equal(e.takeCovers(head(9, 50), true), 'version');
  assert.equal(e.takeCovers(head(9, 50), false), 'start-again');
  assert.equal(e.takeCovers(new Uint8Array(0), true), 'failed');
  assert.equal(e.takeCovers(new Uint8Array(0), false), 'start-again');
  assert.equal(e.coversRev(), 42, 'what was refused moves nothing');
});

test('paused is the crate\'s to say', async () => {
  const e = await engine();
  assert.equal(e.paused({ ...held, lost: 2 }), false);
  assert.equal(e.paused({ ...held, lost: 3 }), true);
});
