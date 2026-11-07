// The page's tape head (lib/tape/head.ts, decision 0102 as amended): the app's TapeHead.kt read the same way. A
// track made up here, at 1000 frames a second so its numbers are easy to follow: its left channel is the frame's own
// number, its right the same negated (Catmull-Rom on a straight line is the line, so every read is exact).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { ahead, Chunks, cubic, CHUNK, KEPT, ASKED_MOST, Marks, Reader } = await import('../src/lib/tape/head.ts');

const RATE = 1000;
/** A track of `chunks` chunks, all decoded unless `missing` says which are not. */
function track(chunks: number, missing: number[] = []) {
  const frames = chunks * CHUNK;
  const c = new Chunks(frames);
  for (let k = 0; k < chunks; k++) if (!missing.includes(k)) c.put(k, chunkOf(k));
  return c;
}
function chunkOf(k: number) {
  const a = new Int16Array(CHUNK * 2);
  for (let i = 0; i < CHUNK; i++) {
    const j = (k * CHUNK + i) % 30000;
    a[i * 2] = j;
    a[i * 2 + 1] = -j;
  }
  return a;
}
/** Reads `frames` frames in blocks of 128, as the worklet does; the output, and why it ended if it did. */
function read(r: InstanceType<typeof Reader>, frames: number) {
  const left: number[] = [];
  const right: number[] = [];
  let ended = null;
  for (let n = 0; n < frames && !ended; n += 128) {
    const l = new Float32Array(128);
    const rr = new Float32Array(128);
    ended = r.fill(l, rr);
    left.push(...l);
    right.push(...rr);
  }
  return { left, right, ended };
}
const near = (a: number, b: number, what: string) => assert.ok(Math.abs(a - b) < 1e-6, `${what}: ${a} ≠ ${b}`);

test('Catmull-Rom as the app has it: through its middle points, a line stays a line', () => {
  near(cubic(1, 2, 3, 4, 0), 2, 'at its start');
  near(cubic(1, 2, 3, 4, 1), 3, 'at its end');
  near(cubic(1, 2, 3, 4, 0.25), 2.25, 'a line');
});

test('asks ahead the way it goes, as far as its speed needs', () => {
  // At 1×, 600 ms ahead: ceil(0.6 × 44100 / 8192) = 4 chunks past the one it is in.
  assert.deepEqual(ahead(0, 1, 0, 44100), [0, 1, 2, 3, 4]);
  assert.deepEqual(ahead(10 * CHUNK + 5, -1, 0, 44100), [10, 9, 8, 7, 6]);
  // At least two ahead, however slow.
  assert.deepEqual(ahead(0, 0.1, 0, RATE), [0, 1, 2]);
  // Above the top: the chunk it is in and the next, then those under the piece playing and the next one.
  const fast = ahead(0, 32, 0, 44100);
  assert.deepEqual(fast.slice(0, 2), [0, 1]);
  assert.equal(fast.length, 2 + 2 * (Math.ceil((16 * 0.35 * 44100) / CHUNK) + 1));
});

test('chunks: wanted nearest first, the farthest dropped, the least recently read let go', () => {
  const c = new Chunks(10 * CHUNK);
  c.want([3, 4, -1, 99]);
  c.want([1, 2]);
  assert.equal(c.take(), 1, 'the newest wants first');
  assert.equal(c.take(), 2);
  assert.equal(c.take(), 3, 'then those asked before');
  c.want([1, 4]);
  assert.equal(c.take(), 4, 'one being decoded is not asked again');
  assert.equal(c.take(), undefined, 'none past the track, none below its start');
  const many = new Chunks(1000 * CHUNK);
  many.want(Array.from({ length: 300 }, (_, i) => i));
  let n = 0;
  while (many.take() !== undefined) n++;
  assert.equal(n, ASKED_MOST);
  const kept = new Chunks(1000 * CHUNK);
  for (let k = 0; k < KEPT; k++) kept.put(k, new Int16Array(2));
  kept.get(0);
  kept.put(KEPT, new Int16Array(2));
  assert.equal(kept.size, KEPT);
  assert.ok(kept.get(0), 'read lately: kept');
  assert.equal(kept.get(1), undefined, 'the least recently read: let go');
});

test('at 1× it reads the track as it is, after its fade in', () => {
  const r = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 1000, speed: () => 1, gain: 1 });
  const { left, right } = read(r, 512);
  near(left[0], (1000 * 0.125) / 32768, 'its fade in: an eighth at its first frame');
  near(left[300], 1300 / 32768, 'left');
  near(right[300], -1300 / 32768, 'right');
});

test('backwards it reads the track the other way', () => {
  const r = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 2000, speed: () => -1, gain: 1 });
  const { left } = read(r, 512);
  near(left[300], 1700 / 32768, 'three hundred frames back');
});

test('the output’s own rate: a frame given is the track’s rate over the output’s', () => {
  const r = new Reader({ chunks: track(2), rate: 44100, outRate: 48000, fromMs: 0, speed: () => 1, gain: 1 });
  read(r, 1280);
  near(r.at, (1280 * 44100) / 48000, 'where it reads');
});

test('a chunk not there is silence faded out, never a wait, and back once it comes', () => {
  const c = track(2, [0]);
  const r = new Reader({ chunks: c, rate: RATE, outRate: RATE, fromMs: 1000, speed: () => 1, gain: 1 });
  const first = read(r, 256);
  assert.ok(first.left.every((x) => x === 0), 'silence');
  assert.ok(c.misses > 0, 'counted');
  c.put(0, chunkOf(0));
  const then = read(r, 256);
  near(then.left[255], (1000 + 511) / 32768, 'back at full once its fades are through');
});

test('its volume follows its speed below a quarter, to silence at a stand', () => {
  const slow = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 1000, speed: () => 0.125, gain: 1 });
  const { left } = read(slow, 512);
  near(left[400], ((1000 + 400 * 0.125) * 0.5) / 32768, 'half at an eighth');
  const stand = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 1000, speed: () => 0, gain: 1 });
  assert.ok(read(stand, 512).left.every((x) => x === 0), 'silence at a stand');
});

test('above the top it is heard in pieces, each read at the top from where the head had got', () => {
  const r = new Reader({ chunks: track(4), rate: RATE, outRate: RATE, fromMs: 0, speed: () => 32, gain: 1 });
  const { left } = read(r, 1024);
  // The second piece begins at 350 ms, where the head had got at 32×: frame 11200; 100 ms in, it reads at 16×.
  near(left[450], (11200 + 16 * 100) / 32768, 'the second piece, a hundred ms in');
  near(left[350], 0, 'a piece begins from silence (its crossfade)');
});

test('past the track’s end it fades out and says so', () => {
  const r = new Reader({ chunks: track(1), rate: RATE, outRate: RATE, fromMs: CHUNK - 100, speed: () => 1, gain: 1 });
  assert.equal(read(r, 1024).ended, 'end');
  const s = new Reader({ chunks: track(1), rate: RATE, outRate: RATE, fromMs: 100, speed: () => -1, gain: 1 });
  assert.equal(read(s, 1024).ended, 'start');
});

test('stopped, it fades out and ends', () => {
  const r = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 1000, speed: () => 1, gain: 1 });
  read(r, 256);
  r.stop();
  assert.equal(read(r, 256).ended, 'stopped');
});

test('where it is heard: the frames played, through what each block was read from', () => {
  const r = new Reader({ chunks: track(2), rate: RATE, outRate: RATE, fromMs: 1000, speed: () => -1, gain: 1 });
  read(r, 256);
  near(r.marks.heardMs(64)!, 936, 'sixty-four frames back from a second');
  near(r.marks.heardMs(200)!, 800, 'in the second block');
  assert.equal(r.marks.heardSpeed(200), -1);
  assert.equal(new Marks(RATE, RATE).heardMs(10), undefined, 'none before any block');
});
