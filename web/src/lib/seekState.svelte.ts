// Holding a track's position to seek (decision 0019, as Finamp's slider), shared by the full player and the mini
// player (the owner, 2026-10-26: one waveform control, the same everywhere; lib/SeekWave draws it): the handle and the
// time follow the finger and the player seeks only on release. Made in a player's script (its effects belong to it).
//
// - While held, where it plays stays marked, still moving (the owner, 2026-10-24: "something to aim for"); held within
//   SNAP_PX of it, the handle snaps onto it, and let go there nothing is sought. Only while the finger is near the bar
//   (SNAP_AWAY_PX above or below it): further off it comes loose, so a seek just beside where it plays can still be made.
// - The lens and the white line come only after LENS_AFTER ms held (the owner, 2026-10-26): a tap shows only where it
//   goes, in the waveform's colours.
// - Let go, the bar shows where it was let go until the player is there (`sought`), then hands over to where it plays
//   in one step (the owner, 2026-10-25: the takeover atomic); never there (refused, another track), back after 3 s.
// - Let go over the Cancel button (the owner, 2026-10-26), nothing is sought.
// - The lens zooms by the finger (the owner, 2026-10-28): after LENS_ZOOM_AFTER ms held, from 3x finer while it drags
//   fast to 24x as it slows and while it holds still, on a geometric curve and eased on it (zoom reads in ratios); and the
//   finger moves the point as finely as the lens shows it (at 24x an eighth as far per px as at 3x), from where it is:
//   what is seen there can be set there.

import { untrack } from 'svelte';
import { rewindNow } from './ramping.svelte';

const SNAP_PX = 12;
const SNAP_AWAY_PX = 40;
const LENS_AFTER = 100;
/** The lens's zoom (× the waveform): dragging fast, held still; the finger's speed (px/s) at which it is least. */
export const LENS_Z_MIN = 3;
const LENS_Z_MAX = 24;
const LENS_FAST_PX_S = 300;
/** Held this long before it zooms in at all (the owner, 2026-10-28: most seeks are done within 2 s); easing in (slowing
 *  down) and out (speeding up), ms. */
const LENS_ZOOM_AFTER = 2000;
const LENS_IN_MS = 450;
const LENS_OUT_MS = 200;

export type Lens = { width: number; middle: number; waveHeight: number; min: number; max: number };

export class Seek {
  /** Where the handle is held (seconds), while the finger is down; only then. */
  held = $state<number | null>(null);
  /** Where it was let go, until the player is there. */
  sought = $state<number | null>(null);
  snapped = $state(false);
  lensOn = $state(false);
  /** Over a waveform when the finger went down (a waveform coming in then takes over on release, not under it). */
  shapedHeld = $state<boolean | null>(null);
  lens = $state<Lens | null>(null);
  /** The finger is over the Cancel button. */
  overCancel = $state(false);
  /** The lens's zoom now (× the waveform). */
  lensZoom = $state(LENS_Z_MIN);
  private width = 0;
  /** The finger: where it last was (px), when, how fast it goes (px/s, smoothed); the held start; the zoom's frames. */
  private lastX: number | null = null;
  private lastAt = 0;
  private speed = 0;
  private downAt = 0;
  private zoomRaf = 0;
  private lensTimer: ReturnType<typeof setTimeout> | undefined;
  private soughtTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    readonly now: () => number,
    private dur: () => number | null,
    private seekTo: (secs: number) => void,
  ) {
    // There: within half a second of it (a player reports in steps).
    $effect(() => {
      const s = this.sought;
      if (s !== null && Math.abs(this.now() - s) <= 0.5) untrack(() => (this.sought = null));
    });
    $effect(() => () => {
      clearTimeout(this.lensTimer);
      clearTimeout(this.soughtTimer);
      cancelAnimationFrame(this.zoomRaf);
    });
  }

  /** What the bar and the time show (seconds): over Cancel, where it plays (let go there, that is what stays; the
   *  owner, 2026-10-26: the waveform answers the Cancel button). */
  get pos() {
    // A seek that winds: where the wind is, from the moment it is asked (the owner, 2026-10-27), never the place it goes.
    if (this.held === null && (rewindNow.on || rewindNow.asked)) return this.now();
    return this.snapped || this.overCancel ? this.now() : (this.held ?? this.sought ?? this.now());
  }

  /** Held at `secs`, `away` px above or below the bar: on the playing position (snapped) or not. */
  hold(secs: number, away = 0) {
    const dur = this.dur();
    const near = !!dur && this.width > 0 && away <= SNAP_AWAY_PX && (Math.abs(secs - this.now()) / dur) * this.width <= SNAP_PX;
    if (near && !this.snapped) navigator.vibrate?.(8);
    this.snapped = near;
    this.held = secs;
  }

  /** Where along `el` the pointer is, in seconds. */
  at(x: number, el: HTMLElement) {
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (x - r.left) / r.width)) * (this.dur() ?? 0);
  }

  /** The finger down on the bar `el` (over a waveform or not); `room`: the player's edges, for the lens. */
  begin(x: number, el: HTMLElement, shaped: boolean, room: HTMLElement | undefined) {
    clearTimeout(this.lensTimer);
    this.lensOn = false;
    this.lensTimer = setTimeout(() => (this.lensOn = true), LENS_AFTER);
    this.shapedHeld = shaped;
    const r = el.getBoundingClientRect();
    this.width = r.width;
    this.hold(this.at(x, el));
    const box = room?.getBoundingClientRect() ?? r;
    const w = el.querySelector('.wave')?.getBoundingClientRect();
    this.lens = shaped && w ? { width: r.width, middle: r.height / 2, waveHeight: w.height, min: box.left - r.left + 75, max: box.right - r.left - 75 } : null;
    // The finger's speed and the lens's zoom from here.
    this.lastX = x;
    this.lastAt = this.downAt = performance.now();
    this.speed = 0;
    this.lensZoom = LENS_Z_MIN;
    cancelAnimationFrame(this.zoomRaf);
    if (this.lens) {
      let last = performance.now();
      const step = (t: number) => {
        const dt = t - last;
        last = t;
        // Not moved for a moment: still.
        if (t - this.lastAt > 60) this.speed *= Math.exp(-dt / 120);
        // Geometric: each step slower the same factor finer (a straight line would crowd the range at the slow end).
        const slow = t - this.downAt < LENS_ZOOM_AFTER ? 0 : Math.max(0, 1 - this.speed / LENS_FAST_PX_S);
        const target = LENS_Z_MIN * (LENS_Z_MAX / LENS_Z_MIN) ** slow;
        // Eased on the same scale.
        const k = 1 - Math.exp(-dt / (target > this.lensZoom ? LENS_IN_MS : LENS_OUT_MS));
        this.lensZoom = Math.exp(Math.log(this.lensZoom) + (Math.log(target) - Math.log(this.lensZoom)) * k);
        this.zoomRaf = requestAnimationFrame(step);
      };
      this.zoomRaf = requestAnimationFrame(step);
    }
  }

  /** The finger moved, at `x`, `away` px off the bar. */
  moveTo(x: number, el: HTMLElement, away: number) {
    if (this.held === null) return;
    const t = performance.now();
    const dx = this.lastX === null ? 0 : x - this.lastX;
    this.speed += ((Math.abs(dx) * 1000) / Math.max(1, t - this.lastAt) - this.speed) * (1 - Math.exp(-(t - this.lastAt) / 120));
    this.lastX = x;
    this.lastAt = t;
    // With the lens: from where it is, as finely as the lens shows it; else straight under the finger.
    const dur = this.dur() ?? 0;
    if (!this.lens || !this.lensOn || !this.width || !dur) return this.hold(this.at(x, el), away);
    const secs = this.held + (dx / this.width) * dur * (LENS_Z_MIN / this.lensZoom);
    this.hold(Math.min(dur, Math.max(0, secs)), away);
  }

  /** Nothing sought: back to where it plays. */
  letGo() {
    clearTimeout(this.lensTimer);
    cancelAnimationFrame(this.zoomRaf);
    this.lensZoom = LENS_Z_MIN;
    this.lastX = null;
    this.lensOn = false;
    this.held = null;
    this.snapped = false;
    this.shapedHeld = null;
    this.lens = null;
    this.overCancel = false;
  }

  /** Let go at `secs` (or where it is held): on the playing position or over Cancel, nothing; else seek there. */
  end(secs = this.held) {
    if (this.held === null || secs === null) return this.letGo();
    if (this.snapped || this.overCancel) return this.letGo();
    this.release(secs);
  }

  /** Seek, the bar on it until the player is there. */
  release(secs: number) {
    this.letGo();
    this.sought = secs;
    this.seekTo(secs);
    clearTimeout(this.soughtTimer);
    this.soughtTimer = setTimeout(() => (this.sought = null), 3000);
  }
}
