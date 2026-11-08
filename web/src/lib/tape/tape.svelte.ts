// The tape head on the page (decision 0102, as amended), as the app's TapeHead.kt is in the app: the playing track's
// file fetched whole as it starts ([load]) and opened by two decoding workers (lib/tape/decoder.worker), read by the
// audio worklet at the engine's speed either way ([start], lib/tape/head.worklet), where the sound heard is read off
// the output's own timestamp through what each block was read from ([heardMs]), never the clock. Only in a browser:
// in the app its own player plays.
import tapeUrl from '../wasm/tape.wasm?url';
import { Marks, WORKERS, madeIn, type Ended, type Mark, type Motion, type TapeWasm } from './head';
import type { DeckWasm } from './deck';
import workletUrl from './head.worklet.ts?worker&url';

export type { Motion };

/** The file opened and ready to read (the id it was asked by: a copy has its own, lib/tape/file), the one on its way, and
 *  the opened one's length; where a reading let go will stand (ms of the track; null before a let-go). */
export const tape = $state({ ready: null as string | null, loading: null as string | null, lengthMs: null as number | null, standsAt: null as number | null, by: null as string | null });

/** A reading's let-go as the head took it (its motion's ms, the speed it brakes to, the brake's least length): carried
 *  on into the next track with the motion. */
export type LetGo = { atMs: number; to: number; brakeMs: number };
let letGone: LetGo | null = null;
/** What a reading is told once it is heard to have done what it is for (a tap landed, a spin-up at its speed, a let-go
 *  at its new speed), and the timer to the ear's frame it is heard at. */
let onDone: (() => void) | null = null;
let doneTimer: ReturnType<typeof setTimeout> | null = null;

let ctx: AudioContext | null = null;
let node: AudioWorkletNode | null = null;
let out: GainNode | null = null;
let workers: Worker[] = [];
let marks: Marks | null = null;
let ended: ((why: Ended, motionMs: number) => void) | null = null;

/** tape.wasm's bytes and its instance here (the deck's machine, and what a let-go would come to), made once. */
let bytesMade: Promise<ArrayBuffer> | null = null;
let mine: (TapeWasm & DeckWasm) | null = null;
const engineBytes = () => (bytesMade ??= fetch(tapeUrl).then((r) => r.arrayBuffer()));

/** The engine on the page's own thread: the deck's machine is in it (lib/tape/deck). */
export async function engine(): Promise<TapeWasm & DeckWasm> {
  if (mine) return mine;
  const bytes = await engineBytes();
  mine ??= (await WebAssembly.instantiate(bytes.slice(0))).instance.exports as unknown as TapeWasm & DeckWasm;
  return mine;
}

/** The head's context and its worklet, made once (a context starts suspended until a press resumes it). */
async function head(): Promise<{ ctx: AudioContext; node: AudioWorkletNode; out: GainNode }> {
  if (ctx && node && out) return { ctx, node, out };
  const c = new AudioContext({ latencyHint: 'playback' });
  await c.audioWorklet.addModule(workletUrl);
  const n = new AudioWorkletNode(c, 'cratarr-tape-head', { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2] });
  const o = c.createGain();
  n.connect(o).connect(c.destination);
  let engineReady: (ok: boolean) => void = () => {};
  const worklet = new Promise<boolean>((resolve) => (engineReady = resolve));
  n.port.onmessage = (e: MessageEvent) => {
    const m = e.data;
    if (m?.type === 'marks') for (const k of m.marks as Mark[]) marks?.add(k);
    if (m?.type === 'stands-at') {
      tape.standsAt = typeof m.place === 'number' && m.when >= 0 ? m.place : null;
      if (letGone && typeof m.atMs === 'number') letGone.atMs = m.atMs;
    }
    if (m?.type === 'done' && typeof m.frame === 'number') doneHeardAt(m.frame);
    if (m?.type === 'ended') ended?.(m.why as Ended, typeof m.motionMs === 'number' ? m.motionMs : 0);
    if (m?.type === 'engine') engineReady(m.ready === true);
  };
  // The engine (tape.wasm), instantiated in the worklet from its bytes (no fetch there), there before any reading; and
  // here too (the deck's machine, what a let-go would come to: [landingIf]).
  await engine();
  const bytes = (await engineBytes()).slice(0);
  n.port.postMessage({ type: 'engine', bytes }, [bytes]);
  if (!(await worklet)) throw new Error('the tape engine did not start');
  ctx = c;
  node = n;
  out = o;
  return { ctx: c, node: n, out: o };
}

/** The head's output, for what listens to it (the visuals, levelling): none before the first track. */
export const tapeOutput = (): { context: AudioContext; node: AudioNode } | null => (ctx && out ? { context: ctx, node: out } : null);

/**
 * The file `id` at `url` fetched whole and opened, the one before let go (the playing track only); asked again for
 * the same file, nothing. Null once ready, else what went wrong.
 */
export async function load(id: string, url: string): Promise<string | null> {
  if (tape.ready === id || tape.loading === id) return null;
  tape.loading = id;
  tape.ready = null;
  tape.lengthMs = null;
  stop();
  try {
    const r = await fetch(url);
    if (!r.ok) throw new Error(`Cratarr said ${r.status}`);
    const bytes = await r.arrayBuffer();
    if (tape.loading !== id) return null;
    const { node } = await head();
    for (const w of workers) w.terminate();
    workers = Array.from({ length: WORKERS }, () => new Worker(new URL('./decoder.worker.ts', import.meta.url), { type: 'module' }));
    const channels = workers.map(() => new MessageChannel());
    // Each worker its own copy of the file (no memory shared without cross-origin isolation); the last one takes it.
    const opened = await Promise.all(
      workers.map(
        (w, i) =>
          new Promise<{ rate: number; frames: number; by: string }>((resolve, reject) => {
            w.onmessage = (e: MessageEvent) => (e.data?.type === 'opened' ? resolve(e.data) : reject(new Error(e.data?.error ?? 'not opened')));
            const own = i === workers.length - 1 ? bytes : bytes.slice(0);
            w.postMessage({ type: 'open', id, bytes: own, port: channels[i].port2 }, [own, channels[i].port2]);
          }),
      ),
    );
    if (tape.loading !== id) return null;
    const { rate, frames } = opened[0];
    const ports = channels.map((c) => c.port1);
    node.port.postMessage({ type: 'track', frames, rate, ports }, ports);
    marks = new Marks(rate, ctx!.sampleRate);
    tape.ready = id;
    tape.loading = null;
    tape.lengthMs = (frames * 1000) / rate;
    tape.by = opened[0].by;
    return null;
  } catch (e) {
    if (tape.loading === id) tape.loading = null;
    return e instanceof Error ? e.message : String(e);
  }
}

/**
 * Reads the opened track from `fromMs` at `motion` (the engine's) at `gain`; `onEnded` once it stopped (by [stop], or
 * at either end of the track), with where its motion had got (its ms); `done` once it is heard to have done what it is
 * for (a tap landed, a spin-up at its speed, a let-go at its new speed). `carried`: a motion carried on from the track
 * before, from that ms of it, let go there already or not. False where the track is not ready.
 */
export function start(
  id: string,
  fromMs: number,
  motion: Motion,
  gain: number,
  onEnded: (why: Ended, motionMs: number) => void,
  done: () => void,
  carried?: { motionAt: number; letGo: LetGo | null },
): boolean {
  if (tape.ready !== id || !ctx || !node) return false;
  void ctx.resume();
  marks?.clear();
  tape.standsAt = null;
  doneOff();
  onDone = done;
  letGone = carried?.letGo ?? null;
  ended = onEnded;
  node.port.postMessage({ type: 'start', fromMs, motion, gain, motionAt: carried?.motionAt ?? 0, letGo: carried?.letGo ?? undefined });
  return true;
}

/** The reading's motion let go (now, by the frames the head read, unless `atMs` of its own time is given): it brakes
 *  from its own speed to `to` × (`brakeMs` at least, the engine's brake); [tape]'s `standsAt` says where it will stand,
 *  and the reading's `done` is told when it is heard at its new speed. */
export function letGo(to: number, brakeMs: number, atMs?: number) {
  letGone = { atMs: atMs ?? -1, to, brakeMs };
  if (doneTimer) clearTimeout(doneTimer);
  doneTimer = null;
  node?.port.postMessage({ type: 'let-go', atMs, to, brakeMs });
}

/** The reading's let-go as the head took it, for carrying it on into the next track; none before one. */
export const letGoMade = (): LetGo | null => (letGone && letGone.atMs >= 0 ? { ...letGone } : null);

/** The reading's `done` told when the ear reaches the context's `frame` (the output's own timestamp, not the clock). */
function doneHeardAt(frame: number) {
  if (doneTimer) clearTimeout(doneTimer);
  const wait = () => {
    const p = played();
    if (!ctx || p === null || p >= frame) {
      doneTimer = null;
      const d = onDone;
      onDone = null;
      d?.();
      return;
    }
    doneTimer = setTimeout(wait, Math.max(1, ((frame - p) * 1000) / ctx.sampleRate));
  };
  wait();
}

/** Nothing more told of the reading's done. */
function doneOff() {
  if (doneTimer) clearTimeout(doneTimer);
  doneTimer = null;
  onDone = null;
}

/** Its gain from now on (the page's volume, ducking, the track's level), followed without a click. */
export function setGain(gain: number) {
  node?.port.postMessage({ type: 'gain', gain });
}

/**
 * Where a reading would stand were its motion let go at `motionMs` (to `to` ×, `brakeMs` at least), and when (its
 * motion's ms): the engine asked here, on a motion of the same shape, nothing changed in the reading. The reading began
 * at `fromMs` of the track at its motion's `motionAt`. None without the engine yet.
 */
export function landingIf(motion: Motion, motionAt: number, fromMs: number, motionMs: number, to: number, brakeMs: number): { place: number; when: number } | null {
  const x = mine;
  if (!x) return null;
  const h = madeIn(x, motion);
  if (!h) return null;
  try {
    x.motion_let_go(h, motionMs, to, brakeMs);
    const when = x.motion_stands_at(h);
    if (when < 0) return null;
    return { place: fromMs + x.motion_gone(h, when) - x.motion_gone(h, motionAt), when };
  } finally {
    x.motion_free(h);
  }
}

/** The reading stopped, faded out first (no click); nothing more told of it. */
export function stop() {
  doneOff();
  node?.port.postMessage({ type: 'stop' });
}

/** The context's frame heard now: the output's own timestamp, else its time less what it says lies before the ear. */
function played(): number | null {
  if (!ctx) return null;
  const t = ctx.getOutputTimestamp?.().contextTime ?? ctx.currentTime - (ctx.baseLatency ?? 0) - (ctx.outputLatency ?? 0);
  return Math.round(t * ctx.sampleRate);
}

/** Where the sound heard now is (ms of the track); none while nothing was read. */
export function heardMs(): number | undefined {
  const p = played();
  return p === null ? undefined : marks?.heardMs(p);
}

/** How fast the sound heard now was read (× the track's, signed); none while nothing was read. */
export function heardSpeed(): number | undefined {
  const p = played();
  return p === null ? undefined : marks?.heardSpeed(p);
}

/** The track let go (another is chosen, or the page plays no more). */
export function release() {
  stop();
  for (const w of workers) w.terminate();
  workers = [];
  tape.ready = null;
  tape.loading = null;
  tape.lengthMs = null;
}
