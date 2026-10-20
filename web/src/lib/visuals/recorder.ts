// The picture lab's recordings (decision 0057, step 3; development only): each frame's inputs to the renderer and the
// compositor, kept as plain data (tests/pictures) for picture-lab.html to replay through two pipelines and compare
// pixel by pixel. They were recorded from the page before the switch; the page on the store records no more (its
// recorder wrapped the renderer's own methods, and went with it), so these are the ones there are.
import type { Flame, Xform } from './flame';
import type { FrameInfo, Module, PatchExtra, SignalRef } from './patch';
import { shownOf } from './chain-params';
import type { FlameRenderer, Shown } from './render';

type Plain<T> = T extends Map<infer K, infer V> ? [K, V][] : T extends Float32Array ? number[] : T extends object ? { [P in keyof T]: Plain<T[P]> } : T;
export type RecordedXform = Plain<Xform>;
export type RecordedFlame = Plain<Flame>;

/** What the compositor was given in a frame: the extras, every module's knobs, the frame (its signals as they were
 *  read: by the reference's JSON), the flame layer's share and its morph's target. */
export type RecordedCompose = {
  extra: PatchExtra;
  knobs: Record<string, Record<string, number>>;
  frame: { dt: number; time: number; beat: FrameInfo['beat']; signals: Record<string, number>; waveform: number[] | null };
  flame: number;
  flameTo?: number;
};

export type RecordedFrame = {
  /** The clock (ms) as the frame was drawn. */
  at: number;
  passes: number;
  look: Plain<Shown>;
  flame: RecordedFlame;
  /** The screen's size, and the buffer's. */
  w: number;
  h: number;
  bw: number;
  bh: number;
  points: number;
  compose: RecordedCompose | null;
  /** The extras the compositor let go of what was not in, before this frame. */
  pruned?: PatchExtra;
};

export type Recording = { v: 1; name: string; points: number; frames: RecordedFrame[] };

/** What the lab sets to record: how many frames, and where they go; `done` once there are that many. */
export type RecordRequest = { name: string; max: number; recording?: Recording; done?: boolean };

const plain = <T>(x: T): Plain<T> =>
  JSON.parse(JSON.stringify(x, (_, v) => (v instanceof Map ? [...v] : ArrayBuffer.isView(v) ? Array.from(v as Float32Array) : v)));

/** The recorded flame as the renderer takes it. */
export const flameOf = (f: RecordedFlame): Flame => {
  const xform = (x: RecordedXform): Xform => ({ ...x, variations: new Map(x.variations) }) as Xform;
  return { ...f, xforms: f.xforms.map(xform), ...(f.final ? { final: xform(f.final) } : {}) } as Flame;
};

/** The recorded look as the renderer takes it. */
export const lookOf = (l: Plain<Shown>): Shown => l as unknown as Shown;

/** A signal reference's key in a recording. */
export const signalKey = (ref: SignalRef) => JSON.stringify([ref.signal, ref.band ?? null, ref.reference ?? null, ref.speed ?? null]);
