// The map's fourth closeness, Sound (0.15; the owner, 2026-11-02): two playlists that share no track or artist sit
// close when they sound alike; a switch beside shared tracks, genre and artists (lib/proximity).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { key, rankers, weigh } = await import('../src/lib/proximity.ts');

const playlists = ['Deep', 'Warm', 'Loud'].map((name) => ({ name, tracks: 10, genres: [], focus: null }));
const link = (a: number, b: number, sound: number) => ({ a, b, tracks: 0, genre: 0, artists: 0, sound, shared_tracks: 0, shared_artists: 0 });
const data = { playlists, links: [link(0, 1, 0.9), link(0, 2, 0.1)] } as unknown as Parameters<typeof rankers>[0];

test('sound alone ties playlists that share nothing else, the closest sounding closest', () => {
  const w = weigh(data, rankers(data), { tracks: false, genre: false, artists: false, sound: true });
  assert.ok((w.get(key(0, 1)) ?? 0) > (w.get(key(0, 2)) ?? 0));
});

test('with sound off, they share nothing', () => {
  const w = weigh(data, rankers(data), { tracks: true, genre: true, artists: true, sound: false });
  assert.equal(w.size, 0);
});
