// What a list asks for a cover (lib/coverSize; finding 86): the least of a few sizes that fills its box in the device's pixels, of what
// Cratarr makes sizes of, and nothing else.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { COVER_STEPS, sizable, sizedCover } = await import('../src/lib/coverSize.ts');

const RECORD = '/api/albums/cratarr/al-1/cover?v=abc';

test('a list asks the least size that fills its box', () => {
  assert.equal(sizedCover(RECORD, 64), '/api/albums/cratarr/al-1/cover?v=abc&size=160');
  assert.equal(sizedCover(RECORD, 160), '/api/albums/cratarr/al-1/cover?v=abc&size=160');
  assert.equal(sizedCover(RECORD, 161), '/api/albums/cratarr/al-1/cover?v=abc&size=320');
  assert.equal(sizedCover(RECORD, 641), '/api/albums/cratarr/al-1/cover?v=abc&size=1280');
});

test('a box larger than every size asks for the whole picture', () => {
  assert.equal(sizedCover(RECORD, 1281), RECORD);
});

test("a track's cover takes a size as its record's does", () => {
  assert.equal(sizedCover('/api/cover/cratarr/tr-1', 300), '/api/cover/cratarr/tr-1?size=320');
});

test('the size asked now is the one in the address', () => {
  assert.equal(sizedCover('/api/albums/cratarr/al-1/cover?size=1280', 100), '/api/albums/cratarr/al-1/cover?size=160');
});

test('what Cratarr makes no sizes of is asked as it is', () => {
  for (const src of [
    '/__device/cover/al-1',
    '/__server/cover/s1/c1?size=600',
    'blob:http://page/abc',
    'data:image/jpeg;base64,AAAA',
    'http://elsewhere.test/api/albums/cratarr/al-1/cover',
    '/api/albums/cratarr/al-1',
    '/api/albums/cratarr/al-1/cover/extra',
  ]) {
    assert.equal(sizable(src), false, src);
    assert.equal(sizedCover(src, 100), src, src);
  }
});

test("the least size is the small covers' Cratarr makes for a device, which it then hands out as they are", () => {
  const made = readFileSync(new URL('../../server/src/app/device_covers.rs', import.meta.url), 'utf8').match(/pub const SIDE: u32 = (\d+);/);
  assert.equal(COVER_STEPS[0], Number(made![1]));
});
