// The picture lab's recorder (decision 0057, step 3; development only): each frame's inputs to the renderer and the
// compositor, kept as plain data for picture-lab.html to replay through two pipelines and compare pixel by pixel. On
// only where the lab asks for it (`globalThis.cratarrPictureRecord`), before the visuals open.
import type { Compositor } from './compose';
import type { Flame, Xform } from './flame';
import type { FrameInfo, Module, PatchExtra, SignalRef } from './patch';
import { shownOf } from './chain-params';
import type { FlameRenderer, Shown } from './render';

type Plain<T> = T extends Map<infer K, infer V> ? [K, V][] : T extends Float32Array ? number[] : T extends object ? { [P in keyof T]: Plain<T[P]> } : T;
export type RecordedXform = Plain<Xform>;
export type RecordedFlame = Plain<Flame>;

/** What the compositor was given in a frame: the extras, every module's knobs, the frame (its signals as they were
 *  read: by the reference's JSON), the flame layer's share. */
export type RecordedCompose = {
  extra: PatchExtra;
  knobs: Record<string, Record<string, number>>;
  frame: { dt: number; time: number; beat: FrameInfo['beat']; signals: Record<string, number>; waveform: number[] | null };
  flame: number;
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

/** Records what `renderer` and `compositor` are given, frame by frame, where the lab asks. */
export function recordPictures(renderer: FlameRenderer, compositor: Compositor | null) {
  const want = (globalThis as { cratarrPictureRecord?: RecordRequest }).cratarrPictureRecord;
  if (!want || want.recording) return;
  const rec: Recording = { v: 1, name: want.name, points: renderer.pointCount, frames: [] };
  want.recording = rec;
  let flame: Flame | null = null;
  let pending: RecordedFrame | null = null;
  let pruned: PatchExtra | undefined;
  const setFlame = renderer.setFlame.bind(renderer);
  renderer.setFlame = (f) => {
    flame = f;
    setFlame(f);
  };
  const frame = renderer.frame.bind(renderer);
  renderer.frame = (passes, now, w, h) => {
    if (want.done || !flame) return frame(passes, now, w, h);
    pending = { at: performance.now(), passes, look: plain(shownOf(now)), flame: plain(flame), w, h, bw: renderer.width, bh: renderer.height, points: renderer.pointCount, compose: null, ...(pruned ? { pruned } : {}) };
    pruned = undefined;
    frame(passes, now, w, h);
    rec.frames.push(pending);
    pending = null;
    if (rec.frames.length >= want.max) want.done = true;
  };
  if (!compositor) return;
  const prune = compositor.prune.bind(compositor);
  compositor.prune = (extra) => {
    pruned = plain(extra);
    prune(extra);
  };
  const run = compositor.run.bind(compositor);
  compositor.run = (picture, width, height, extra, knobsOf, info, share = 1) => {
    if (!pending) return run(picture, width, height, extra, knobsOf, info, share);
    const signals: Record<string, number> = {};
    const modules: Module[] = [...extra.layers.flatMap((l) => [l.source, ...l.modules]), ...extra.loop];
    pending.compose = {
      extra: plain(extra),
      knobs: Object.fromEntries(modules.map((m) => [m.id, { ...knobsOf(m) }])),
      frame: { dt: info.dt, time: info.time, beat: info.beat && { ...info.beat }, signals, waveform: info.waveform ? Array.from(info.waveform) : null },
      flame: share,
    };
    const read = info.signals;
    return run(picture, width, height, extra, knobsOf, { ...info, signals: (ref) => (signals[signalKey(ref)] = read(ref)) }, share);
  };
}
