/**
 * How fast a sound is heard going, measured on its own clock (an audio element's `currentTime` against the page's), never
 * the rate set on it (the owner, 2026-11-03: "we need to measure the tape truthfully. No guessing anywhere"; a browser
 * applies a `playbackRate` about 0.1 s late, so a let-go's brake began from a speed not yet heard). Pure.
 */
export class HeardPace {
  /** Its readings: when (ms, the page's clock) and where its sound was (s); oldest first. */
  private readings: [number, number][] = [];

  /** A reading of its clock at `now`: where it was then. A jump back or far on (a seek, a new stream) starts over. */
  read(now: number, at: number): void {
    const last = this.readings[this.readings.length - 1];
    if (last) {
      if (now <= last[0]) return;
      const moved = at - last[1];
      if (moved < 0 || moved > ((now - last[0]) / 1000) * JUMP_X + JUMP_S) this.readings = [];
    }
    this.readings.push([now, at]);
    while (this.readings.length > 2 && now - this.readings[1][0] >= SPAN_MS) this.readings.shift();
  }

  /** Its speed (× its own) over the readings of the last [SPAN_MS] at least; null until it has that long. */
  speed(): number | null {
    const n = this.readings.length;
    if (n < 2) return null;
    const [t0, a0] = this.readings[0];
    const [t1, a1] = this.readings[n - 1];
    if (t1 - t0 < SPAN_MS) return null;
    return (a1 - a0) / ((t1 - t0) / 1000);
  }

  /** Its readings dropped (a new stream on its element). */
  clear(): void {
    this.readings = [];
  }
}

/** The span a speed is measured over (ms): long enough for a browser's clock steps, short enough to follow a brake. */
export const SPAN_MS = 120;
/** A move faster than this (× plus s) between readings is a jump, not playing. */
const JUMP_X = 64;
const JUMP_S = 0.25;

const paces = new WeakMap<HTMLMediaElement, HeardPace>();

/** An element's heard pace, read at `now` (paused: still). */
export function heard(el: HTMLMediaElement, now = performance.now()): HeardPace {
  let p = paces.get(el);
  if (!p) paces.set(el, (p = new HeardPace()));
  p.read(now, el.currentTime);
  return p;
}

/** How fast an element is heard going now (× its own): measured; 0 paused; null before it has been heard long enough. */
export function heardSpeed(el: HTMLMediaElement, now = performance.now()): number | null {
  const s = heard(el, now).speed();
  return el.paused ? 0 : s;
}
