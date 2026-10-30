// A module's face (the owner, 2026-10-20: "a standard panel for every module"): what its node shows live past its
// knobs, built of a few blocks shared by the graph and the panel (face/*.svelte): a Light (a trigger's or a gate's LED,
// flashing on a pulse), a Meter (a level, its peak held), a Scope (a value's last two seconds, a threshold across it),
// a Thumbnail (a picture module's output, small and live), a Number; the ADSR's and the LFO's own shapes. A module
// type declares its face (ports.ts `face`); one that does not gets this file's default by its type, and a module with
// a picture (or light) output a thumbnail of it. Live values come from the engine's outputs (LiveFaces: `module.port`,
// a channel's `module.port.<n>`), else a knob as set. A module inside a device is found by its path, or (as the
// sidechain.ts leversOf does) by the id after its last `/`. Pure.
//
// The meter and the scope follow NexusUI's (MIT, © Ben Taylor): the meter a bar in the accent colour on the panel's
// fill; the scope a line across its width, its stroke `height / 100 + 2`, a centre line while nothing comes. The peak
// hold falls as the meters of Surge XT and Vital (GPL-3.0) do: held a while, then falling at a steady rate. Our code.

import type { FacePart, Ports, PortType } from '../ports';
import type { ModuleType } from '../patch';

export type { FacePart };

/** The defaults, by type, for the types that declare none (the modulators of modulators.ts). */
export const DEFAULT_FACES: Readonly<Record<string, readonly FacePart[]>> = {
  // The beat gate: its input over the last two seconds with Above across it, its light as it opens.
  gate: [
    { kind: 'scope', of: 'in', threshold: 'above' },
    { kind: 'light', of: 'out' },
  ],
  lfo: [{ kind: 'lfo' }],
  random: [{ kind: 'scope', of: 'out' }],
  fast: [{ kind: 'meter', of: 'out', peak: true }],
};

const PICTURES: ReadonlySet<PortType> = new Set<PortType>(['picture', 'light']);

/** A module type's face: declared, else the default for its type, else none. (A picture's or points' preview is not a
 *  face part: one popover shows it, for the module picked: previewOf.) */
export function faceOf(t: (ModuleType & { face?: readonly FacePart[] }) | undefined, _ports?: Ports | null): FacePart[] {
  if (t?.face) return [...t.face];
  if (t && DEFAULT_FACES[t.type]) return [...DEFAULT_FACES[t.type]];
  return [];
}

/** What a module's preview shows (the owner, 71.1: only the module picked): its picture (or light) output through the
 *  renderer's thumbnail; else its points output as dots (no GPU pass); else none. */
export function previewOf(ports: Ports | null | undefined): 'picture' | 'points' | null {
  if (ports?.out.some((o) => PICTURES.has(o.type))) return 'picture';
  if (ports?.out.some((o) => o.type === 'points')) return 'points';
  return null;
}
/** Whose preview shows: the one module picked alone (select mode), else (nothing picked) the one last tapped. */
export function previewTarget(picked: readonly string[], cables: number, tapped: string | null): string | null {
  if (picked.length === 1 && !cables) return picked[0];
  return picked.length || cables ? null : tapped;
}
/** A module's points this frame (the engine's frame, by its path or by its id after the last `/`). */
export function pointsOf(frame: { points?: Readonly<Record<string, readonly { x: number; y: number }[]>> } | null | undefined, path: string): readonly { x: number; y: number }[] | null {
  const all = frame?.points;
  if (!all) return null;
  if (all[path]) return all[path];
  const last = path.slice(path.lastIndexOf('/') + 1);
  if (all[last]) return all[last];
  for (const k in all) if (k.endsWith(`/${last}`)) return all[k];
  return null;
}

/** The parts drawn in the face's own area (a light on an output sits beside that output instead). */
export const areaParts = (parts: readonly FacePart[], outs: readonly string[]) => parts.filter((p) => !(p.kind === 'light' && outs.includes(p.of ?? 'out')));

/** The face's drawing width (a node's narrowest content), a thumbnail's size (16:9), a lane's height. */
export const FACE_W = 130;
export const THUMB_H = 73;
export const LANE_H = 11;
const HEIGHT: Record<FacePart['kind'], number> = { meter: 10, scope: 30, light: 12, thumb: THUMB_H, number: 18, adsr: 26, lfo: 26 };
const GAP = 6;

/** The face area's height: its parts down it, a multi-lane ADSR a lane each. */
export function faceHeight(parts: readonly FacePart[], lanes = 1): number {
  if (!parts.length) return 0;
  return parts.reduce((h, p) => h + (p.kind === 'adsr' && lanes > 1 ? lanes * LANE_H : p.kind === 'meter' && p.channels ? 22 : HEIGHT[p.kind]), 0) + GAP * (parts.length - 1);
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** An output now, for the module at `path` (the devices it is inside, joined by `/`): `path.port.channel`, then
 *  `path.port`, then (its `out`) `path`; where the engine's ids differ from the path, by the id after the last `/`. */
export function liveValue(outputs: Readonly<Record<string, number>> | null | undefined, path: string, port = 'out', channel?: number): number | null {
  if (!outputs) return null;
  const keys = (m: string) => [...(channel === undefined ? [] : [`${m}.${port}.${channel}`]), `${m}.${port}`, ...(port === 'out' && channel === undefined ? [m] : [])];
  for (const k of keys(path)) if (finite(outputs[k])) return outputs[k];
  const last = path.slice(path.lastIndexOf('/') + 1);
  for (const want of keys(last)) {
    if (finite(outputs[want])) return outputs[want];
    for (const k in outputs) if (k.endsWith(`/${want}`) && finite(outputs[k])) return outputs[k];
  }
  return null;
}

/** What a part reads now: its output (each channel's where it has several, else the largest of them), else its knob as
 *  set, else nothing. */
export function partValue(outputs: Readonly<Record<string, number>> | null | undefined, path: string, part: FacePart, knobs: Readonly<Record<string, number>>, channel?: number): number | null {
  const port = part.of ?? 'out';
  const v = liveValue(outputs, path, port, channel);
  if (v !== null) return v;
  if (channel === undefined) {
    const each = Array.from({ length: 6 }, (_, c) => liveValue(outputs, path, port, c)).filter(finite);
    if (each.length) return Math.max(...each);
  }
  return finite(knobs[port]) ? knobs[port] : null;
}

// ---- Meter: its peak held -----------------------------------------------------------------------------------------

/** How long a peak holds (s) and how fast it falls after (of the meter's range a second). */
export const PEAK_HOLD = 1;
export const PEAK_FALL = 0.5;
export type Peak = { value: number; at: number };
/** The peak after a reading `v` at time `t` (s): a higher one taken at once; else held `PEAK_HOLD`, then falling. */
export function peakStep(p: Peak | null, v: number, t: number): Peak {
  if (!p || v >= p.value) return { value: v, at: t };
  const since = t - p.at - PEAK_HOLD;
  if (since <= 0) return p;
  const fell = p.value - since * PEAK_FALL;
  return fell <= v ? { value: v, at: t } : { value: fell, at: t - PEAK_HOLD };
}
/** A meter's fill (0..1) for a value in 0..max; a reduction's (gain taken away: 1 none) from the other end. */
export const meterFill = (v: number | null, max = 1, reduction = false) => {
  if (v === null) return 0;
  const u = Math.min(1, Math.max(0, v / max));
  return reduction ? 1 - u : u;
};

// ---- Scope: the last two seconds of a value -----------------------------------------------------------------------

export const SCOPE_SECONDS = 2;
export type Sample = { t: number; v: number };
/** The samples after one more at `t` (s): the last `SCOPE_SECONDS` kept, none twice at one time. */
export function scopePush(seen: readonly Sample[], v: number | null, t: number): Sample[] {
  if (v === null || !finite(v)) return seen.filter((s) => s.t > t - SCOPE_SECONDS);
  const kept = seen.filter((s) => s.t > t - SCOPE_SECONDS && s.t < t);
  return [...kept, { t, v }];
}
/** The range a scope draws: 0..1, wider where its values or its threshold go past (a bipolar value's from −1). */
export function scopeRange(seen: readonly Sample[], threshold?: number | null): [number, number] {
  const vs = seen.map((s) => s.v);
  const lo = Math.min(0, ...vs) < -1e-6 ? Math.min(-1, ...vs) : 0;
  const hi = Math.max(1, ...vs, ...(finite(threshold) ? [threshold * 1.25] : []));
  return [lo, hi];
}
/** A scope's trace (SVG), the newest sample at the right edge, `SCOPE_SECONDS` across; empty with nothing seen. */
export function scopePath(seen: readonly Sample[], w: number, h: number, range: [number, number], now = seen[seen.length - 1]?.t ?? 0): string {
  if (!seen.length) return '';
  const [lo, hi] = range;
  const y = (v: number) => (h - 1) - ((v - lo) / Math.max(1e-9, hi - lo)) * (h - 2);
  const x = (t: number) => w - ((now - t) / SCOPE_SECONDS) * w;
  const f = (n: number) => Math.round(n * 10) / 10;
  return seen.map((s, i) => `${i ? 'L' : 'M'}${f(x(s.t))},${f(y(s.v))}`).join(' ');
}
/** Where a value sits across a scope's height (its threshold's line). */
export const scopeY = (v: number, h: number, range: [number, number]) => (h - 1) - ((v - range[0]) / Math.max(1e-9, range[1] - range[0])) * (h - 2);
/** NexusUI's oscilloscope stroke: `height / 100 + 2`. */
export const scopeStroke = (h: number) => Math.floor(h / 100 + 2);

// ---- Light: a pulse flashes it ------------------------------------------------------------------------------------

/** Whether a reading is a pulse (a trigger's rising edge through ½), from the one before. */
export const pulsed = (before: number | null, now: number | null) => now !== null && now >= 0.5 && (before === null || before < 0.5);

// ---- Number -------------------------------------------------------------------------------------------------------

/** A value as a Number shows it: as many places as tell it apart, none past 100. */
export function numberText(v: number | null): string {
  if (v === null || !finite(v)) return '–';
  const a = Math.abs(v);
  return (v < 0 ? '−' : '') + a.toFixed(a >= 100 ? 0 : a >= 10 ? 1 : 2);
}

// ---- Thumbnails: a picture module's output, small, one a frame in turn --------------------------------------------

/** What draws a module's output into a canvas (the renderer's `thumbnail(moduleId, canvas)`: the module observed, its
 *  picture at 1/16 size, read back a little late). Null where it shows (or soon will); else why it has none. */
export type Thumbnailer = (module: string, canvas: HTMLCanvasElement) => string | null;
/** The thumbnails a graph shows: each registers its canvas while drawn (and hears why it has no picture, where it has
 *  none); one is drawn a frame, in turn. */
export class Thumbs {
  private list: { module: string; canvas: HTMLCanvasElement; why?: (why: string | null) => void }[] = [];
  private turn = 0;
  add(module: string, canvas: HTMLCanvasElement, why?: (why: string | null) => void): () => void {
    const it = { module, canvas, why };
    this.list.push(it);
    return () => (this.list = this.list.filter((x) => x !== it));
  }
  get size() {
    return this.list.length;
  }
  /** The next one's turn (a frame): drawn by `draw`; the one it was, if any. */
  next(draw: Thumbnailer | undefined): string | null {
    if (!draw || !this.list.length) return null;
    const it = this.list[this.turn++ % this.list.length];
    const why = draw(it.module, it.canvas);
    it.why?.(why);
    return it.module;
  }
}
