// The recorded tracks (tests/fixtures/tracks: four of the library's, their signals and beats as the server sends them)
// and a recorder of the one engine over them, frame by frame, each frame's numbers flattened: what the device gives
// its stages and its meters, and every module's node values. For the tests that hold the engine to itself (a chain in
// a device, cables merged or converted); the golden frames (golden.ts) hold it to its references. (0.10: today's
// summary and the old engine it came from are gone, and the equivalence proofs against them with them.)
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

// settings.svelte.ts and reach.svelte.ts make their state with Svelte's rune at load; outside Svelte it is the value.
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { decode: decodeSignals } = await import('../../src/lib/visuals/signals.ts');
const { decode: decodeBeats } = await import('../../src/lib/visuals/beats.ts');
const { musicOf, newMusicState } = await import('../../src/lib/visuals/music-core.ts');
const { FLUID_FULL } = await import('../../src/lib/visuals/settings.svelte.ts');

import type { EngineFrame } from '../../src/lib/visuals/engine/graph.ts';
import type { Music } from '../../src/lib/visuals/music-core.ts';
import type { VisualSettings } from '../../src/lib/visuals/settings.svelte.ts';
import type { Signals } from '../../src/lib/visuals/signals.ts';
import type { Beats } from '../../src/lib/visuals/beats.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
/** The recorded tracks: a deep-house groove (with beats), an ambient one, a surf-rock one and a Chopin nocturne. */
export const TRACKS = ['tr-10329', 'tr-8934', 'tr-21896', 'tr-6487'] as const;

export type Track = { id: string; signals: Signals; beats: Beats | null };
export function track(id: string): Track {
  const at = (ext: string) => readFileSync(join(HERE, '..', 'fixtures', 'tracks', `${id}.${ext}`));
  const buffer = (b: Buffer) => b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
  const signals = decodeSignals(buffer(at('bands')));
  if (!signals) throw new Error(`${id}: its signals do not decode`);
  const raw = at('beats');
  return { id, signals, beats: raw.length ? decodeBeats(buffer(raw)) : null };
}

/** What an engine is stepped with each frame, and gives: the one engine's frame, however its patch is made. */
export type StepOptions = { still: boolean; now: number; fluidFull: number };
export type Engine = { name: string; step(m: Music, s: VisualSettings, dt: number, opts: StepOptions): EngineFrame };

/** A value's numbers by path, objects and arrays walked. */
function put(out: Map<string, number>, key: string, v: unknown) {
  if (typeof v === 'number') return void out.set(key, v);
  if (typeof v === 'boolean') return void out.set(key, v ? 1 : 0);
  if (!v || typeof v !== 'object') return;
  if (ArrayBuffer.isView(v)) return Array.from(v as unknown as ArrayLike<number>).forEach((x, i) => out.set(`${key}.${i}`, x));
  for (const [k, x] of Object.entries(v as object)) put(out, `${key}.${k}`, x);
}

/** A frame's outcome as named numbers: what the device gives (its stages, its meter), every module's node values (by
 *  its own name, whatever device it sits in: `cratarr/show/tonemap` is `tonemap`). */
export function flatten(f: EngineFrame): Map<string, number> {
  const out = new Map<string, number>();
  put(out, 'device', f.device);
  for (const [id, p] of Object.entries(f.params)) put(out, `params.${id.slice(id.lastIndexOf('/') + 1)}`, p);
  return out;
}

export const FPS = 30;

/** Steps `engine` over `t` from `from` for `seconds` at FPS (musicOf, playing). One map a frame. */
export function record(engine: Engine, t: Track, s: VisualSettings, from = 0, seconds = 30, still = false): Map<string, number>[] {
  const state = newMusicState();
  const dt = 1 / FPS;
  const frames: Map<string, number>[] = [];
  for (let k = 0; k < Math.round(seconds * FPS); k++) {
    const at = from + k * dt;
    const m = musicOf(t.signals, t.beats, at, true, { state, at: at * 1000, lighting: s.lighting });
    frames.push(flatten(engine.step(m, s, dt, { still, now: at * 1000, fluidFull: FLUID_FULL })));
  }
  return frames;
}

/** Where two recordings part: each field's largest difference, and the frame it was at (a field the second lacks:
 *  Infinity). */
export function compare(a: Map<string, number>[], b: Map<string, number>[]): Map<string, { most: number; frame: number }> {
  const out = new Map<string, { most: number; frame: number }>();
  for (let f = 0; f < Math.min(a.length, b.length); f++)
    for (const [k, v] of a[f]) {
      const w = b[f].get(k);
      const d = w === undefined ? Infinity : Math.abs(v - w);
      if (d > (out.get(k)?.most ?? -1)) out.set(k, { most: d, frame: f });
    }
  return out;
}
