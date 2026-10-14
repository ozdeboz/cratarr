// Finamp's motion (decision 0018, read in its source 2026-10-14): the values the player and the menus use, so the
// port moves as Finamp does. Reduced motion turns off what Finamp turns off (the player's open and close, the colour
// and background fades, the cover fade).

/** Flutter's `Easing.legacyDecelerate` (bottom sheets), as a CSS curve and a Svelte easing. */
export const DECELERATE_CSS = 'cubic-bezier(0, 0, 0.2, 1)';
export const decelerate = (t: number) => {
  // cubic-bezier(0, 0, 0.2, 1) solved for y at x = t (Newton's method on x).
  let u = t;
  for (let i = 0; i < 6; i++) {
    const x = 3 * (1 - u) * u * u * 0.2 + u * u * u - t + 3 * (1 - u) * (1 - u) * u * 0;
    const dx = 6 * (1 - u) * u * 0.2 - 3 * u * u * 0.2 + 3 * u * u;
    if (Math.abs(dx) < 1e-6) break;
    u -= x / dx;
  }
  u = Math.min(1, Math.max(0, u));
  return 3 * (1 - u) * u * u * 1 + u * u * u + 3 * (1 - u) * (1 - u) * u * 0;
};

export const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Bottom sheets (menus, the queue): in 250 ms, out 200 ms (Flutter's showModalBottomSheet). */
export const SHEET_IN = 250;
export const SHEET_OUT = 200;
