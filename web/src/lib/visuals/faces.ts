// The graph's live node faces (decision 0057 §4b, as VCV Rack's lights): what a control source is doing now, drawn small
// on its node. Read from the module's outputs only, as a panel's lights read its jacks, so any engine that gives them
// lights them (evaluate.ts `outputs` by module id, the new engine by `module.port`, a channel as `module.port.n`):
// - ADSR: its shape from its knobs, the stage it is in lit, a dot where its output is on that shape; a lane a channel.
// - LFO: its wave from its knobs, a dot at its phase (worked out from its output and which way it is going).
// - Clock: Beat, Bar and Phrase blink, from the frame's beat (bars of 4, phrases of 8 bars) until the Clock gives them.
// (The other faces, and these drawn, are face/'s: the standard panel every module has.)
// Pure; the caller keeps each face's memory from sample to sample.

import type { FrameInfo } from './patch';

/** What the faces are fed (faces.svelte.ts LiveFaces, at most ~30 times a second while the graph shows). */
export type LiveFrame = {
  /** Seconds. */
  time: number;
  beat: FrameInfo['beat'];
  /** Each output now: by module id (one output) or `module.port`, a channel's `module.port.<n>`. */
  outputs: Readonly<Record<string, number>>;
  /** Each points output this frame (the engine's `points`: a Grid's, x and y in 0..1), by module id. */
  points?: Readonly<Record<string, readonly { x: number; y: number }[]>>;
};


const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** A module's output now: `module.port.channel`, `module.port`, or (its `out`) the module's own. */
export function outputOf(outputs: Readonly<Record<string, number>>, module: string, port = 'out', channel?: number): number | null {
  const keys = [...(channel !== undefined ? [`${module}.${port}.${channel}`] : []), `${module}.${port}`, ...(port === 'out' && channel === undefined ? [module] : [])];
  for (const k of keys) if (finite(outputs[k])) return outputs[k];
  return null;
}

// ---- ADSR ------------------------------------------------------------------------------------------------------------

export type Stage = 'idle' | 'attack' | 'decay' | 'sustain' | 'release';
export const STAGES: readonly Exclude<Stage, 'idle'>[] = ['attack', 'decay', 'sustain', 'release'];
export type AdsrKnobs = { attack: number; decay: number; sustain: number; release: number; amount: number };
/** One channel's memory: its stage, its envelope last (0..1), where its release began. */
export type AdsrMemo = { stage: Stage; level: number; from: number };
export const newAdsrMemo = (): AdsrMemo => ({ stage: 'idle', level: 0, from: 0 });

/** A channel's knobs (a polyphonic knob's `<id>.<channel>`, else its one value). */
export function laneKnobs(knobs: Readonly<Record<string, number>>, channel: number): AdsrKnobs {
  const k = (id: string, or: number) => knobs[`${id}.${channel}`] ?? knobs[id] ?? or;
  return { attack: k('attack', 0.04), decay: k('decay', 0), sustain: k('sustain', 0), release: k('release', 0.2), amount: k('amount', 1) };
}

/** The shape across `w`: where each stage starts and ends (attack, decay, the sustain's plateau, release), each its
 *  time's share with a least share so a short one still shows; the plateau a third of the times'. */
export function adsrSpans(k: AdsrKnobs, w: number): Record<Exclude<Stage, 'idle'>, [number, number]> {
  const t = [k.attack, k.decay, 0, k.release].map((v) => Math.max(0, finite(v) ? v : 0));
  t[2] = (t[0] + t[1] + t[3]) / 3 || 1;
  const sum = t.reduce((a, b) => a + b, 0) || 1;
  const least = 0.1;
  const share = t.map((v) => Math.max(least, v / sum));
  const all = share.reduce((a, b) => a + b, 0);
  let x = 0;
  const spans = share.map((s) => {
    const span: [number, number] = [x, x + (s / all) * w];
    x = span[1];
    return span;
  });
  return { attack: spans[0], decay: spans[1], sustain: spans[2], release: spans[3] };
}

/** The shape's corners in a `w` × `h` box (up is more): its start, the peak, the sustain's start and end, its end; the
 *  stage `i` (STAGES) runs from corner i to corner i + 1. */
export function adsrPoints(k: AdsrKnobs, w: number, h: number, y0 = 0): [number, number][] {
  const s = adsrSpans(k, w);
  const sy = y0 + h - clamp(k.sustain, 0, 1) * h;
  return [
    [0, y0 + h],
    [s.attack[1], y0],
    [s.decay[1], sy],
    [s.sustain[1], sy],
    [w, y0 + h],
  ];
}

const f1 = (v: number) => Math.round(v * 10) / 10;
const line = (pts: [number, number][]) => pts.map(([x, y], i) => `${i ? 'L' : 'M'}${f1(x)},${f1(y)}`).join(' ');

/** The shape as a path, and one stage of it (the lit one). */
export const adsrPath = (k: AdsrKnobs, w: number, h: number, y0 = 0) => line(adsrPoints(k, w, h, y0));
export function adsrStagePath(k: AdsrKnobs, stage: Stage, w: number, h: number, y0 = 0): string {
  const i = STAGES.indexOf(stage as (typeof STAGES)[number]);
  return i < 0 ? '' : line(adsrPoints(k, w, h, y0).slice(i, i + 2));
}

/** The stage a channel is in from its output now, as its lights would show it: rising, the attack; falling from the
 *  peak toward the sustain, the decay; held at the sustain, the sustain; falling under it, the release; nothing, idle. */
export function adsrStep(memo: AdsrMemo, k: AdsrKnobs, out: number | null): AdsrMemo {
  const e = out !== null && k.amount > 0 ? clamp(out / k.amount, 0, 1) : 0;
  if (e < 0.003) return { stage: 'idle', level: 0, from: 0 };
  const s = clamp(k.sustain, 0, 1);
  const d = e - memo.level;
  let stage = memo.stage;
  let from = memo.from;
  if (d > 0.002) stage = 'attack';
  else if (d < -0.002) {
    const falling = memo.stage === 'attack' || memo.stage === 'decay';
    if (falling && e > s + 0.01) stage = 'decay';
    else if (falling && Math.abs(e - s) < 0.02) stage = 'sustain';
    else {
      if (memo.stage !== 'release') from = memo.level;
      stage = 'release';
    }
  } else if (Math.abs(e - s) < 0.02 && memo.stage !== 'release') stage = 'sustain';
  else if (memo.stage === 'idle') stage = 'sustain';
  return { stage, level: e, from: stage === 'release' ? Math.max(from, e) : 0 };
}

/** Where a channel's dot is on the shape (none idle): along its stage by how far its output has gone through it. */
export function adsrDot(memo: AdsrMemo, k: AdsrKnobs, w: number, h: number, y0 = 0): { x: number; y: number } | null {
  if (memo.stage === 'idle') return null;
  const sp = adsrSpans(k, w);
  const s = clamp(k.sustain, 0, 1);
  const e = memo.level;
  const along = (span: [number, number], t: number) => span[0] + (span[1] - span[0]) * clamp(t, 0, 1);
  const x =
    memo.stage === 'attack'
      ? along(sp.attack, e)
      : memo.stage === 'decay'
        ? along(sp.decay, s < 1 ? (1 - e) / (1 - s) : 1)
        : memo.stage === 'sustain'
          ? along(sp.sustain, 0.5)
          : along(sp.release, memo.from > 0 ? 1 - e / memo.from : 1);
  return { x, y: y0 + h - e * h };
}

// ---- LFO -------------------------------------------------------------------------------------------------------------

export type LfoKnobs = { shape: number; amount: number };
export type LfoMemo = { value: number; rising: boolean };
export const newLfoMemo = (): LfoMemo => ({ value: 0, rising: true });

/** The LFO's wave at phase `p` (a cycle 0..1): a sine at Shape 0 to a triangle at 1 (modulators.ts lfo). */
export function lfoWave(p: number, shape: number): number {
  const sine = Math.sin(2 * Math.PI * p);
  const triangle = 4 * Math.abs(p - 0.25 - Math.floor(p - 0.25) - 0.5) - 1;
  return sine + (triangle - sine) * clamp(shape, 0, 1);
}

/** The phase (0..1) an output `u` (−1..1, Amount taken out) is at, going up or down: two phases give each value, the
 *  way it is going tells which. */
export function lfoPhase(u: number, rising: boolean, shape: number): number {
  const v = clamp(u, -1, 1);
  const sine = Math.asin(v) / (2 * Math.PI);
  const tri = v / 4;
  const q = sine + (tri - sine) * clamp(shape, 0, 1);
  const p = rising ? q : 0.5 - q;
  return p - Math.floor(p);
}

export function lfoStep(memo: LfoMemo, out: number | null): LfoMemo {
  const v = out ?? 0;
  const rising = v > memo.value + 1e-4 ? true : v < memo.value - 1e-4 ? false : memo.rising;
  return { value: v, rising };
}

/** The wave as a path over a `w` × `h` box, a cycle across. */
export function lfoPath(shape: number, w: number, h: number, points = 40): string {
  const mid = h / 2;
  let d = '';
  for (let i = 0; i <= points; i++) {
    const p = i / points;
    d += `${i ? 'L' : 'M'}${Math.round(p * w * 10) / 10},${Math.round((mid - lfoWave(p, shape) * mid * 0.85) * 10) / 10} `;
  }
  return d.trim();
}

/** The dot: at the phase the output says, on the wave. */
export function lfoDot(memo: LfoMemo, k: LfoKnobs, w: number, h: number): { x: number; y: number } {
  const u = k.amount > 0 ? memo.value / k.amount : 0;
  const p = lfoPhase(u, memo.rising, k.shape);
  const mid = h / 2;
  return { x: p * w, y: mid - lfoWave(p, k.shape) * mid * 0.85 };
}

// ---- Clock -----------------------------------------------------------------------------------------------------------

/** A bar's beats and a phrase's bars, until the Clock's divider and multiplier give its own. */
export const BAR_BEATS = 4;
export const PHRASE_BARS = 8;
/** How long a light takes to fade (seconds). */
const BLINK = 0.12;

/** How lit Beat, Bar and Phrase are (0..1): each flashes on its beat and fades. */
export function clockLights(beat: FrameInfo['beat']): { beat: number; bar: number; phrase: number } {
  if (!beat) return { beat: 0, bar: 0, phrase: 0 };
  const on = Math.exp(-Math.max(0, beat.since) / BLINK);
  const bar = beat.index % BAR_BEATS === 0 ? on : 0;
  return { beat: on, bar, phrase: beat.index % (BAR_BEATS * PHRASE_BARS) === 0 ? on : 0 };
}
