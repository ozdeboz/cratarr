// Sync play in a browser (decision 0037, amended 2026-10-23: the owner wanted the ThinkBook in the tablet's group): a
// page follows a group, never leads one. It reads the group's sound as stamped records (`/api/groups/{leader}/
// listen.pcm`, Snapcast::frames) and plays each at its moment on the browser's audio clock: the stamp, plus the group's
// buffer, less its speaker delay, on Cratarr's time (a clock exchange with `/api/time`), at the moment the sound leaves
// the speaker (`getOutputTimestamp`). An `<audio>` element held about 3 s before it played, more than the group's 1 s,
// and each skip to bring it back left a gap it rebuffered from (the owner heard dropouts every 10 s). Pieces that follow
// each other within 5 ms are laid end to end; further apart, the page takes the stamp's moment (a resync). What it
// heard unevenly goes to its log (lib/playOnTarget): pieces come too late to play, gaps, resyncs, a lost stream.
import { pageVolume } from './localPlayer.svelte';

type Log = (level: 'info' | 'warn', what: string, fields?: Record<string, unknown>) => void;

const CLOCK_MS = 60_000;
/** Its sound lost (an error, the stream ended): opened again after this long. */
const REOPEN_MS = 2_000;
/** How often what went unevenly is written down (when anything did). */
const TALLY_MS = 10_000;
/** Pieces this close (s) are laid end to end. */
const JOIN_S = 0.005;

export const listening = $state({ leader: null as string | null, playing: false, suspended: false });

let ctx: AudioContext | null = null;
let gain: GainNode | null = null;
let reading: AbortController | null = null;
let player = '';
let log: Log = () => {};
let clockTimer: ReturnType<typeof setInterval> | undefined;
let tallyTimer: ReturnType<typeof setInterval> | undefined;
let reopenTimer: ReturnType<typeof setTimeout> | undefined;

/** This browser's steady time (epoch ms). */
const nowMs = () => performance.timeOrigin + performance.now();
/** Cratarr's time less this browser's (ms), from the exchange with the shortest round trip. */
let offset = 0;

/** Where the last piece laid ends (audio clock, s); 0: none yet. */
let next = 0;
/** The pieces laid and not yet played, by when they start (audio clock, s). */
let laid: { at: number; src: AudioBufferSourceNode }[] = [];
let lastPieceAt = 0;
const tally = { late: 0, gaps: 0, gapMs: 0, resyncs: 0, pieces: 0 };
/** This minute's drift: how late each piece laid end to end is against its stamp (ms; − early), and the resyncs. Written
 *  down once a minute ("group sound drift") to learn how far a page drifts before choosing how to correct it (the
 *  owner, 2026-10-23: Snapcast corrects from 0.1 ms at most 0.05%; this jumps beyond 5 ms). */
const minute = { lags: [] as number[], resyncs: 0 };
const DRIFT_MS = 60_000;

async function measureClock() {
  let best = Infinity;
  let was = offset;
  for (let i = 0; i < 5; i++) {
    try {
      const t0 = nowMs();
      const r = (await (await fetch('/api/time', { cache: 'no-store' })).json()) as { now_ms: number };
      const t1 = nowMs();
      if (t1 - t0 < best) {
        best = t1 - t0;
        offset = r.now_ms - (t0 + t1) / 2;
      }
    } catch {
      /* the next round */
    }
  }
  if (best < Infinity && Math.abs(offset - was) > 5) log('info', 'group clock', { offset_ms: Math.round(offset), rtt_ms: Math.round(best) });
}

/** The audio clock's time (s) at which sound leaves the speaker at this browser's time `ms`. */
function audioTimeAt(ms: number): number {
  const c = ctx!;
  const ts = c.getOutputTimestamp?.();
  const at = ts?.contextTime ?? c.currentTime;
  const perf = ts?.performanceTime ?? performance.now();
  return at + (ms - (performance.timeOrigin + perf)) / 1000;
}

type Format = { rate: number; channels: number; bits: number; bufferMs: number; delayMs: number };

/** PCM (little-endian, signed) as an AudioBuffer. */
function bufferOf(f: Format, pcm: Uint8Array): AudioBuffer | null {
  const bytes = f.bits / 8;
  const frames = Math.floor(pcm.length / (bytes * f.channels));
  if (!frames) return null;
  const b = ctx!.createBuffer(f.channels, frames, f.rate);
  const v = new DataView(pcm.buffer, pcm.byteOffset, pcm.length);
  for (let ch = 0; ch < f.channels; ch++) {
    const out = b.getChannelData(ch);
    for (let i = 0; i < frames; i++) {
      const at = (i * f.channels + ch) * bytes;
      out[i] =
        bytes === 2
          ? v.getInt16(at, true) / 32768
          : bytes === 4
            ? v.getInt32(at, true) / 2147483648
            : ((v.getUint8(at) | (v.getUint8(at + 1) << 8) | (v.getInt8(at + 2) << 16)) / 8388608);
    }
  }
  return b;
}

/** One piece of the group's sound, stamped `stampUs` (Cratarr's time), laid at its moment. */
function lay(f: Format, stampUs: number, pcm: Uint8Array) {
  const c = ctx!;
  const b = bufferOf(f, pcm);
  if (!b) return;
  lastPieceAt = performance.now();
  tally.pieces++;
  let at = audioTimeAt(stampUs / 1000 + f.bufferMs - f.delayMs - offset);
  if (next) {
    const off = at - next;
    if (Math.abs(off) <= JOIN_S) {
      minute.lags.push((next - at) * 1000);
      at = next;
    } else if (off > 0) {
      tally.gaps++;
      tally.gapMs += off * 1000;
    } else {
      tally.resyncs++;
      minute.resyncs++;
    }
  }
  const now = c.currentTime;
  if (at + b.duration <= now + 0.01) {
    tally.late++;
    next = 0;
    return;
  }
  const src = c.createBufferSource();
  src.buffer = b;
  src.connect(gain!);
  // Partly past: its rest, now.
  if (at < now) src.start(now, now - at);
  else src.start(at);
  next = at + b.duration;
  laid.push({ at, src });
  src.onended = () => (laid = laid.filter((l) => l.src !== src));
}

/** A break (the group silenced at once): what is laid from its moment on is not played. */
function silence(f: Format, stampUs: number) {
  const from = audioTimeAt(stampUs / 1000 + f.bufferMs - f.delayMs - offset);
  for (const l of laid) if (l.at >= from) l.src.stop();
  laid = laid.filter((l) => l.at < from);
  next = 0;
}

/** Reads the group's sound until it ends or is let go. */
async function read(signal: AbortSignal) {
  const leader = listening.leader!;
  const res = await fetch(`/api/groups/${encodeURIComponent(leader)}/listen.pcm?player=${encodeURIComponent(player)}`, { signal, cache: 'no-store' });
  if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
  const reader = res.body.getReader();
  let held = new Uint8Array(0);
  let f: Format | null = null;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) throw new Error('the stream ended');
    const all = new Uint8Array(held.length + value.length);
    all.set(held);
    all.set(value, held.length);
    let at = 0;
    const v = new DataView(all.buffer);
    if (!f) {
      if (all.length < 16) {
        held = all;
        continue;
      }
      f = { rate: v.getUint32(0, true), channels: v.getUint16(4, true), bits: v.getUint16(6, true), bufferMs: v.getUint32(8, true), delayMs: v.getUint32(12, true) };
      at = 16;
      next = 0;
      log('info', 'group sound opened', {
        leader,
        rate: f.rate,
        channels: f.channels,
        bits: f.bits,
        buffer_ms: f.bufferMs,
        delay_ms: f.delayMs,
        output_latency_ms: Math.round((ctx!.outputLatency ?? 0) * 1000),
        base_latency_ms: Math.round(ctx!.baseLatency * 1000),
      });
    }
    while (all.length - at >= 13) {
      const kind = v.getUint8(at);
      const stamp = Number(v.getBigInt64(at + 1, true));
      const len = v.getUint32(at + 9, true);
      if (all.length - at - 13 < len) break;
      if (kind === 1) lay(f, stamp, all.subarray(at + 13, at + 13 + len));
      else if (kind === 2) silence(f, stamp);
      at += 13 + len;
    }
    held = all.slice(at);
  }
}

function open() {
  clearTimeout(reopenTimer);
  reading?.abort();
  if (!listening.leader) return;
  const ctl = new AbortController();
  reading = ctl;
  read(ctl.signal).catch((e) => {
    if (ctl.signal.aborted) return;
    log('warn', 'group sound lost', { error: e instanceof Error ? e.message : String(e) });
    for (const l of laid) l.src.stop();
    laid = [];
    next = 0;
    reopenTimer = setTimeout(open, REOPEN_MS);
  });
}

function volume() {
  if (gain) gain.gain.value = (pageVolume.level / 100) * (pageVolume.ducked ? 0.25 : 1);
}

/** What went unevenly since the last time, written down; whether it plays, as its reports say. */
function check() {
  listening.suspended = ctx?.state === 'suspended';
  listening.playing = ctx?.state === 'running' && performance.now() - lastPieceAt < 1500;
}
function writeDrift() {
  const l = minute.lags.sort((a, b) => a - b);
  if (l.length) {
    const ms = (v: number) => Math.round(v * 10) / 10;
    log('info', 'group sound drift', { median_ms: ms(l[l.length >> 1]), min_ms: ms(l[0]), max_ms: ms(l[l.length - 1]), readings: l.length, resyncs: minute.resyncs });
  }
  minute.lags = [];
  minute.resyncs = 0;
}

function writeTally() {
  if (tally.late || tally.gaps || tally.resyncs)
    log('warn', 'group sound uneven', { pieces: tally.pieces, late: tally.late, gaps: tally.gaps, gap_ms: Math.round(tally.gapMs), resyncs: tally.resyncs });
  Object.assign(tally, { late: 0, gaps: 0, gapMs: 0, resyncs: 0, pieces: 0 });
}

/** Follows `leader`'s group as player `name`, writing to `to` what it hears unevenly. Whether it sounds already
 *  (else the browser wants a tap first: `resumeGroup`). */
export async function followGroup(leader: string, name: string, to: Log): Promise<boolean> {
  if (listening.leader === leader && player === name && ctx) return ctx.state === 'running';
  leaveGroup();
  listening.leader = leader;
  player = name;
  log = to;
  ctx = new AudioContext({ latencyHint: 'playback' });
  gain = ctx.createGain();
  gain.connect(ctx.destination);
  volume();
  await measureClock();
  clockTimer = setInterval(measureClock, CLOCK_MS);
  let ticks = 0;
  tallyTimer = setInterval(() => {
    check();
    if (++ticks % (TALLY_MS / 1000) === 0) writeTally();
    if (ticks % (DRIFT_MS / 1000) === 0) writeDrift();
  }, 1000);
  open();
  if (ctx.state !== 'running') await ctx.resume().catch(() => {});
  check();
  if (ctx.state !== 'running') log('warn', 'group sound held back', { why: 'the browser wants a tap first' });
  return ctx.state === 'running';
}

/** After the tap a browser asks for. */
export async function resumeGroup() {
  await ctx?.resume().catch(() => {});
  check();
}

/** Out of the group: its sound stops. */
export function leaveGroup() {
  if (listening.leader) {
    writeTally();
    writeDrift();
    log('info', 'group sound closed', { leader: listening.leader });
  }
  listening.leader = null;
  listening.playing = false;
  listening.suspended = false;
  clearInterval(clockTimer);
  clearInterval(tallyTimer);
  clearTimeout(reopenTimer);
  reading?.abort();
  reading = null;
  laid = [];
  next = 0;
  ctx?.close().catch(() => {});
  ctx = null;
  gain = null;
}

if (typeof window !== 'undefined') {
  $effect.root(() => {
    $effect(() => {
      void [pageVolume.level, pageVolume.ducked];
      volume();
    });
  });
}
