// The equivalence harness (decision 0057, step 2): today's engine (aspects.ts) and the one engine stepped frame by frame
// over recorded tracks (tests/fixtures/tracks: four of the library's, their signals and beats as the server sends
// them), each frame's whole outcome flattened, so the new engine is proved against today's before the old code goes.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

// settings.svelte.ts and reach.svelte.ts make their state with Svelte's rune at load; outside Svelte it is the value.
(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
const { decode: decodeSignals } = await import('../../src/lib/visuals/signals.ts');
const { decode: decodeBeats } = await import('../../src/lib/visuals/beats.ts');
const { musicOf, newMusicState } = await import('../../src/lib/visuals/music-core.ts');
const { newAspects, stepAspects } = await import('../../src/lib/visuals/aspects.ts');
const { FLUID_FULL } = await import('../../src/lib/visuals/settings.svelte.ts');

import type { AspectsNow } from '../../src/lib/visuals/aspects.ts';
import { ASPECTS } from '../../src/lib/visuals/routes.ts';
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

/** What an engine is stepped with each frame, and gives: today's AspectsNow, whatever runs it. */
export type StepOptions = { still: boolean; now: number; fluidFull: number };
export type Engine = { name: string; step(m: Music, s: VisualSettings, dt: number, opts: StepOptions): AspectsNow };

/** Today's engine. */
export function todays(): Engine {
  const st = newAspects();
  return { name: 'aspects.ts', step: (m, s, dt, opts) => stepAspects(st, m, s, dt, opts) };
}

const WALKS = new Set([ASPECTS.indexOf('look'), ASPECTS.indexOf('behaviour')]);

/** A frame's outcome as named numbers: everything AspectsNow says, each part's and band's apart, the moments as 0/1. */
export function flatten(a: AspectsNow): Map<string, number> {
  const out = new Map<string, number>();
  const put = (k: string, v: number | boolean) => out.set(k, typeof v === 'boolean' ? (v ? 1 : 0) : v);
  // The walks (Look's and Behaviour's columns) left with the Whole frame (0057: Randoms and the Dice now): not compared.
  a.given.forEach((v, i) => WALKS.has(i) || put(`given.${i}`, v));
  for (const k of ['lit', 'glow', 'turn', 'scale', 'keep', 'gamma', 'flash', 'saturation', 'smoothing', 'floor', 'fluid', 'warp', 'tunnel', 'effects', 'facets', 'dive'] as const) put(k, a[k]);
  a.rates.forEach((v, i) => put(`rates.${i}`, v));
  a.shape.forEach((v, i) => put(`shape.${i}`, v));
  a.hits.forEach((v, i) => put(`hits.${i}`, v));
  a.parts.forEach((p, i) => {
    for (const k of ['turn', 'size', 'morph', 'hue', 'spread', 'weight'] as const) put(`parts.${i}.${k}`, p[k] ?? 0);
  });
  for (const [k, v] of Object.entries(a.levels)) put(`levels.${k}`, v);
  for (const [k, v] of Object.entries(a.moments)) put(`moments.${k}`, v);
  a.routed.amounts.forEach((v, i) => put(`routed.amounts.${i}`, v));
  a.routed.knobs.forEach((v, i) => put(`routed.knobs.${i}`, v));
  return out;
}

export const FPS = 30;

/** Steps `engine` over `t` from `from` for `seconds` at FPS, as the precheck does (musicOf, playing). One map a frame. */
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

/** Where two recordings part: each field's largest difference, and the frame it was at. */
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
