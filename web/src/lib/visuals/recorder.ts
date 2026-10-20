// The picture lab's recorder (decision 0057, step 3; development only): each frame's inputs to the renderer and the
// compositor, kept as plain data for picture-lab.html to replay through two pipelines and compare pixel by pixel. On
// only where the lab asks for it (`globalThis.cratarrPictureRecord`), before the visuals open.
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

/** Records what `renderer` is given, frame by frame, where the lab asks. */
export function recordPictures(renderer: FlameRenderer) {
  const want = (globalThis as { cratarrPictureRecord?: RecordRequest }).cratarrPictureRecord;
  if (!want || want.recording) return;
  const rec: Recording = { v: 1, name: want.name, points: renderer.pointCount, frames: [] };
  want.recording = rec;
  let flame: Flame | null = null;
  // The extras changed: the next frame's go with it (the frozen compositor lets go of what is not in them).
  let pruned = false;
  const setFlame = renderer.setFlame.bind(renderer);
  renderer.setFlame = (f) => {
    flame = f;
    setFlame(f);
  };
  const prune = renderer.prune.bind(renderer);
  renderer.prune = (extra) => {
    pruned = true;
    prune(extra);
  };
  const frame = renderer.frame.bind(renderer);
  renderer.frame = (passes, now, w, h, inputs = null) => {
    if (want.done || !flame || !now) return frame(passes, now, w, h, inputs);
    // The page's extras (v1) as the lab replays them; a store's inputs go on unrecorded.
    const extras = inputs && 'extra' in inputs ? inputs : null;
    const pending: RecordedFrame = { at: performance.now(), passes, look: plain(shownOf(now)), flame: plain(flame), w, h, bw: renderer.width, bh: renderer.height, points: renderer.pointCount, compose: null, ...(pruned && extras ? { pruned: plain(extras.extra) } : {}) };
    pruned = false;
    // The compose step's inputs, its signals as they are read.
    let given = inputs;
    if (extras) {
      const signals: Record<string, number> = {};
      const modules: Module[] = [...extras.extra.layers.flatMap((l) => [l.source, ...l.modules]), ...extras.extra.loop];
      const info = extras.frame;
      pending.compose = {
        extra: plain(extras.extra),
        knobs: Object.fromEntries(modules.map((m) => [m.id, { ...extras.knobsOf(m) }])),
        frame: { dt: info.dt, time: info.time, beat: info.beat && { ...info.beat }, signals, waveform: info.waveform ? Array.from(info.waveform) : null },
        flame: extras.flame,
        ...(extras.flameTo !== undefined ? { flameTo: extras.flameTo } : {}),
      };
      given = { ...extras, frame: { ...info, signals: (ref) => (signals[signalKey(ref)] = info.signals(ref)) } };
    }
    frame(passes, now, w, h, given);
    rec.frames.push(pending);
    if (rec.frames.length >= want.max) want.done = true;
  };
}
