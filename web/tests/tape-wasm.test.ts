// The tape head's two WebAssembly modules as the page calls them (decision 0102, as amended; a5's builds): decode.wasm
// through lib/tape/decoder (a file's bytes in, any stretch read out, on from the last or sought), and tape.wasm's
// motions as the worklet calls them (a steady speed, a hold doubling to its top, a let-go braking to a stand).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { decoded } = await import('../src/lib/tape/decoder.ts');
const wasm = (name: string) => new WebAssembly.Module(readFileSync(new URL(`../src/lib/wasm/${name}.wasm`, import.meta.url)));

/** A WAV of `frames` frames at 44.1 kHz, 16-bit stereo: left the frame's number (mod 20000), right the same negated. */
function wavBytes(frames: number): ArrayBuffer {
  const b = new DataView(new ArrayBuffer(44 + frames * 4));
  const text = (at: number, s: string) => [...s].forEach((c, i) => b.setUint8(at + i, c.charCodeAt(0)));
  text(0, 'RIFF');
  b.setUint32(4, 36 + frames * 4, true);
  text(8, 'WAVE');
  text(12, 'fmt ');
  b.setUint32(16, 16, true);
  b.setUint16(20, 1, true);
  b.setUint16(22, 2, true);
  b.setUint32(24, 44100, true);
  b.setUint32(28, 44100 * 4, true);
  b.setUint16(32, 4, true);
  b.setUint16(34, 16, true);
  text(36, 'data');
  b.setUint32(40, frames * 4, true);
  for (let i = 0; i < frames; i++) {
    b.setInt16(44 + i * 4, i % 20000, true);
    b.setInt16(46 + i * 4, -(i % 20000), true);
  }
  return b.buffer;
}
const frameAt = (a: Int16Array, i: number) => [a[i * 2], a[i * 2 + 1]];

test('decode.wasm: a file opened from its bytes, its rate and length', async () => {
  const d = await decoded(wavBytes(88200), wasm('decode'));
  assert.equal(d.rate, 44100);
  assert.equal(d.frames, 88200);
  d.close();
});

test('decode.wasm: a stretch read as it is, on from the last, and sought', async () => {
  const d = await decoded(wavBytes(88200), wasm('decode'));
  const first = d.read(0, 8192);
  assert.equal(first.length, 8192 * 2);
  assert.deepEqual(frameAt(first, 1), [1, -1]);
  assert.deepEqual(frameAt(first, 8191), [8191, -8191]);
  const next = d.read(8192, 8192);
  assert.deepEqual(frameAt(next, 0), [8192, -8192], 'on from the last');
  const far = d.read(50_000, 100);
  assert.deepEqual(frameAt(far, 0), [50_000 % 20000, -(50_000 % 20000)], 'sought');
  const back = d.read(100, 10);
  assert.deepEqual(frameAt(back, 0), [100, -100], 'sought back');
  const end = d.read(88_100, 8192);
  assert.equal(end.length, 100 * 2, 'fewer at its end');
  d.close();
});

test('decode.wasm: bytes it cannot read are refused, the room given back', async () => {
  await assert.rejects(() => decoded(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]).buffer, wasm('decode')));
});

test('tape.wasm: a steady motion, a hold doubling to its top, a let-go braking to a stand', async () => {
  const x = (await WebAssembly.instantiate(wasm('tape'))).exports as unknown as Record<string, (...a: number[]) => number>;
  const s = x.motion_steady(-1.5);
  assert.equal(x.motion_speed(s, 0), -1.5);
  assert.equal(x.motion_speed(s, 10_000), -1.5);
  x.motion_free(s);

  const h = x.motion_hold(2, 16, 500, 1);
  assert.ok(h > 0, 'a handle');
  const early = x.motion_speed(h, 0);
  const later = x.motion_speed(h, 3000);
  assert.ok(early >= 2 - 1e-9 && later <= 16 + 1e-9 && later >= early, `${early} then ${later}`);
  assert.equal(x.motion_stands_at(h), -1, 'no stand before a let-go');
  x.motion_let_go(h, 3000, 0, 300);
  const at = x.motion_speed(h, 3000);
  let last = at;
  for (let t = 3000; t <= 13_000; t += 50) {
    const v = x.motion_speed(h, t);
    assert.ok(v <= last + 1e-9, `never faster after letting go: ${v} after ${last} at ${t}`);
    last = v;
  }
  assert.ok(Math.abs(last) < 1e-9, `at a stand: ${last}`);
  assert.ok(x.motion_stands_at(h) >= 0, 'where it stands');
  x.motion_free(h);
});
