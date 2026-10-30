// A circle drawn with a finger or the mouse (the owner, 2026-10-19: on the full player's cover it shows the overview):
// the path turns around its own middle by most of a full turn, either way, and is wide and round enough to be meant
// (a swipe is a line, a tap a point).

/** How far the path must turn, in degrees. */
const TURN = 300;
/** The least width and height of the path (px), and how much wider than high (or higher than wide) it may be. */
const SIZE = 50;
const ROUND = 3;

export function isCircle(points: readonly { x: number; y: number }[]): boolean {
  if (points.length < 8) return false;
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const w = Math.max(...xs) - Math.min(...xs);
  const h = Math.max(...ys) - Math.min(...ys);
  if (w < SIZE || h < SIZE || w > ROUND * h || h > ROUND * w) return false;
  const cx = xs.reduce((a, b) => a + b, 0) / xs.length;
  const cy = ys.reduce((a, b) => a + b, 0) / ys.length;
  let turned = 0;
  let before = Math.atan2(points[0].y - cy, points[0].x - cx);
  for (const p of points.slice(1)) {
    const now = Math.atan2(p.y - cy, p.x - cx);
    let d = now - before;
    if (d > Math.PI) d -= 2 * Math.PI;
    if (d < -Math.PI) d += 2 * Math.PI;
    turned += d;
    before = now;
  }
  return (Math.abs(turned) * 180) / Math.PI >= TURN;
}
