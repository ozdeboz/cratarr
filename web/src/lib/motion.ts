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

/** Motion reduced: by the system, or Animations switched off on this device (Display; lib/prefs sets `data-motion`). */
export const reducedMotion = () => document.documentElement.dataset.motion === 'off' || matchMedia('(prefers-reduced-motion: reduce)').matches;

// Animations off (the owner, 2026-10-25): every transition of Svelte's ends at once, the ones with a duration of their
// own too. Svelte runs them as Web Animations (Element.animate); with Animations off each runs for no time, so what
// comes shows and what goes is gone at once, and Svelte's own bookkeeping (its end, the element's removal) still runs.
if (typeof Element !== 'undefined') {
  const animate = Element.prototype.animate;
  Element.prototype.animate = function (keyframes, options) {
    if (document.documentElement.dataset.motion === 'off') {
      options = { ...(typeof options === 'number' ? { duration: options } : options), duration: 0, delay: 0 };
    }
    return animate.call(this, keyframes, options);
  };
}

/** Bottom sheets (menus, the queue): in 250 ms, out 200 ms (Flutter's showModalBottomSheet). */
export const SHEET_IN = 250;
export const SHEET_OUT = 200;
