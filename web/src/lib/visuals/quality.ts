// Quality (decision 0050): one ceiling in place of Points, Steps and Resolution, on the same ladder the device's pace
// walks (steps, resolution, points), so the two never disagree. 60 % is exactly what the defaults were before 65
// (2^19 points, 3 steps, the screen's default resolution); the pace takes it lower where frames come late and climbs
// back no higher. Pure.

/** Quality before anyone moves it: today's defaults. */
export const QUALITY = 0.6;

/** The pixels the display pass draws by default: about a megapixel. The points fall into a buffer of at most 0.6 MP
 *  anyway; drawing that out to every pixel of a large screen cost the most of a frame on integrated graphics (5.5 ms of
 *  22 at 2560 x 1600, measured 2026-10-17) and showed little more. */
const AUTO_PIXELS = 1.2e6;

/** This screen's default resolution: up to `AUTO_PIXELS` of its own pixels (0.25..1 of them). */
export function autoResolution(devicePixels: number): number {
  return Math.min(1, Math.max(0.25, Math.sqrt(AUTO_PIXELS / Math.max(1, devicePixels))));
}

/** What Quality `q` (0..1) lets the device draw at most on a screen of `devicePixels`:
 *  - points (log2): 16 + 5q, so 64k at 0, 512k at 60 %, 2M at all of it;
 *  - steps: 1 at 0, 3 at 60 %, 6 at all of it, whole;
 *  - resolution (of the screen's own pixels): 60 % of its default at 0 (the pace's own lowest), the default at 60 %,
 *    then on to all of the screen's pixels at 100 % (beyond the default only where the screen has them to give). */
export function ceilingOf(q: number, devicePixels: number): { points: number; steps: number; resolution: number } {
  const v = Math.min(1, Math.max(0, q));
  const auto = autoResolution(devicePixels);
  const below = Math.min(1, v / QUALITY);
  const above = Math.max(0, (v - QUALITY) / (1 - QUALITY));
  return {
    points: 16 + 5 * v,
    steps: Math.round(1 + 2 * below + 3 * above),
    resolution: v <= QUALITY ? auto * (0.6 + 0.4 * below) : auto + (1 - auto) * above,
  };
}
