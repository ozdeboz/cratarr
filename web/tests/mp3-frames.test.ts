// A Bandcamp preview's MP3 read from its frame headers (lib/mp3Frames), for the waveform the device works out and what
// the player shows of the stream.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { frames, binFrames, scaled } = await import('../src/lib/mp3Frames.ts');

/** `n` frames of MPEG-1 Layer III, 128 kb/s at 44.1 kHz (417 bytes each), `mono` or stereo, after `before` bytes. */
function mp3(n: number, { mono = false, before = new Uint8Array() } = {}) {
  const out = new Uint8Array(before.length + n * 417);
  out.set(before);
  for (let i = 0; i < n; i++) out.set([0xff, 0xfb, 0x90, mono ? 0xc0 : 0x00], before.length + i * 417);
  return out;
}

test('the frames and the stream as their headers say', () => {
  const m = frames(mp3(10))!;
  assert.equal(m.starts.length, 10);
  assert.deepEqual(m.starts.slice(0, 3), [0, 417, 834]);
  assert.equal(m.end, 4170);
  assert.deepEqual([m.sampleRate, m.channels, m.bitrateKbps, m.samplesPerFrame], [44100, 2, 128, 1152]);
  assert.equal(frames(mp3(3, { mono: true }))!.channels, 1);
});

test('an ID3v2 tag and stray bytes before the first frame are passed over', () => {
  // A tag of 20 bytes (syncsafe size 10), then a stray 0xFF that is no frame.
  const tag = new Uint8Array([0x49, 0x44, 0x33, 4, 0, 0, 0, 0, 0, 10, ...new Array(10).fill(0), 0xff, 0x00]);
  const m = frames(mp3(4, { before: tag }))!;
  assert.equal(m.starts[0], tag.length);
  assert.equal(m.starts.length, 4);
});

test('no MP3, no frames', () => {
  assert.equal(frames(new Uint8Array(2000)), null);
  assert.equal(frames(mp3(1)), null);
});

test('the bins cover every frame once, in order', () => {
  const b = binFrames(1000, 512);
  assert.equal(b[0][0], 0);
  assert.equal(b[511][1], 1000);
  for (let i = 1; i < b.length; i++) assert.equal(b[i][0], b[i - 1][1]);
  assert.deepEqual([...scaled([0, 0.5, 1])], [0, 128, 255]);
});

test('the first frame is one whose next frame follows it, never a sync found by chance (0.10: a seek taken from a false one)', async () => {
  const { firstFrame } = await import('../src/lib/mp3Frames.ts');
  // Bytes that look like an MPEG-2.5 header (11025 Hz) by chance, then real 128 kb/s frames.
  const fake = new Uint8Array([0x12, 0xff, 0xe3, 0x18, 0x40, 0x07, 0x99]);
  const b = mp3(4, { before: fake });
  assert.equal(firstFrame(b), fake.length);
  assert.equal(firstFrame(mp3(4)), 0);
  assert.equal(firstFrame(mp3(1).slice(0, 300)), null, 'too few bytes to say');
});
