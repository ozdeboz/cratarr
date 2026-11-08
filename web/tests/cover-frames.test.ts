// Small covers as they come off the wire (lib/coverFrames): where things end, in whatever pieces the body arrives.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { CoverFrames } = await import('../src/lib/coverFrames.ts');

const u32 = (n: number) => [...new Uint8Array(new Uint32Array([n]).buffer)];
const i64 = (n: number) => [...new Uint8Array(new BigInt64Array([BigInt(n)]).buffer)];
const head = [...new TextEncoder().encode('CRXV'), ...u32(1), ...i64(41)];
const frame = (record: number, jpeg: number[] = []) => [...i64(record), ...u32(jpeg.length), ...jpeg];
const body = new Uint8Array([...head, ...frame(1, [10, 11, 12]), ...frame(2), ...frame(3_000_000_000, [13])]);

/** The body read in pieces of `size`: the head and every frame, as they came whole. */
function read(size: number) {
  const f = new CoverFrames();
  let given: Uint8Array | null = null;
  const frames = [];
  for (let at = 0; at < body.length; at += size) {
    f.push(body.slice(at, at + size));
    given = f.head() ?? given;
    if (given) frames.push(...f.take());
  }
  return { f, given, frames };
}

test('in one piece: the head, then each frame, a record of no length having no cover', () => {
  const { f, given, frames } = read(body.length);
  assert.deepEqual([...given!], head);
  assert.deepEqual(frames.map((x) => x.record), [1, 2, 3_000_000_000]);
  assert.deepEqual([...frames[0].jpeg!], [10, 11, 12]);
  assert.equal(frames[1].jpeg, null);
  assert.deepEqual([...frames[2].jpeg!], [13]);
  assert.equal(f.cut, false);
});

test('whatever the pieces, the same frames', () => {
  const whole = read(body.length).frames;
  for (const size of [1, 2, 3, 5, 7, 11, 13, 16, 17, 29]) {
    const { f, frames } = read(size);
    assert.deepEqual(frames, whole, `pieces of ${size}`);
    assert.equal(f.cut, false);
  }
});

test('the head is given once, and not before it is all here', () => {
  const f = new CoverFrames();
  f.push(body.slice(0, 15));
  assert.equal(f.head(), null);
  f.push(body.slice(15, 16));
  assert.ok(f.head());
  assert.equal(f.head(), null);
});

test('no frame is given before it is whole, and what is left says it was cut short', () => {
  const f = new CoverFrames();
  f.push(body.slice(0, -1));
  f.head();
  assert.deepEqual(f.take().map((x) => x.record), [1, 2]);
  assert.equal(f.cut, true);
});

test('a frame\'s head alone is cut short too', () => {
  const f = new CoverFrames();
  f.push(new Uint8Array([...head, ...i64(5)]));
  f.head();
  assert.deepEqual(f.take(), []);
  assert.equal(f.cut, true);
});

test('a head alone holds nothing, and nothing is left', () => {
  const f = new CoverFrames();
  f.push(new Uint8Array(head));
  f.head();
  assert.deepEqual(f.take(), []);
  assert.equal(f.cut, false);
});

test('each frame is given once', () => {
  const f = new CoverFrames();
  f.push(body);
  f.head();
  assert.equal(f.take().length, 3);
  assert.deepEqual(f.take(), []);
});

test('a picture is its own bytes, or a store would keep the whole piece it came in once for each cover', () => {
  const f = new CoverFrames();
  f.push(body);
  f.head();
  const [first] = f.take();
  f.push(new Uint8Array([9, 9, 9]));
  assert.deepEqual([...first.jpeg!], [10, 11, 12]);
  assert.equal(first.jpeg!.buffer.byteLength, 3);
});
