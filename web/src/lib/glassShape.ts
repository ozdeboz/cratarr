// The glass's shapes as lib/glassMotion moves between them (the owner, 2026-11-01): boxes in one frame's coordinates,
// pure so the tests take them as they are.

export type Rect = { x: number; y: number; w: number; h: number };

/** A box from the page's rect of it, in the frame whose page rect is `frame`. */
export const rectIn = (r: { left: number; top: number; width: number; height: number }, frame: { left: number; top: number }): Rect => ({
  x: r.left - frame.left,
  y: r.top - frame.top,
  w: r.width,
  h: r.height,
});

/** The least box holding both. */
export function union(a: Rect, b: Rect): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
}

/** The same to half a pixel. */
export const sameRect = (a: Rect, b: Rect) =>
  Math.abs(a.x - b.x) < 0.5 && Math.abs(a.y - b.y) < 0.5 && Math.abs(a.w - b.w) < 0.5 && Math.abs(a.h - b.h) < 0.5;

/** `t` of the way from one to the other. */
export const between = (a: Rect, b: Rect, t: number): Rect => ({
  x: a.x + (b.x - a.x) * t,
  y: a.y + (b.y - a.y) * t,
  w: a.w + (b.w - a.w) * t,
  h: a.h + (b.h - a.h) * t,
});

const px = (n: number) => `${Math.round(n * 100) / 100}px`;

/** The shape `s` as a clip-path of an element whose box is `b` (both in one frame), its corners rounded `radius`: past
 *  the box where the shape is larger (an inset below 0 draws outside it). */
export const insetOf = (s: Rect, b: Rect, radius: number) =>
  `inset(${px(s.y - b.y)} ${px(b.x + b.w - (s.x + s.w))} ${px(b.y + b.h - (s.y + s.h))} ${px(s.x - b.x)} round ${px(radius)})`;

/** `b` grown to `u`, as the CSS `inset` of an absolutely placed layer whose own box is `b` (top right bottom left). */
export const grownTo = (u: Rect, b: Rect) => `${px(u.y - b.y)} ${px(b.x + b.w - (u.x + u.w))} ${px(b.y + b.h - (u.y + u.h))} ${px(u.x - b.x)}`;
