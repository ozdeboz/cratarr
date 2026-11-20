// A panel's own motion (BACK; the owner, 2026-11-20: "going back from a panel to a page, it would show the panel open transition in
// reverse"): the tablet's side panel and the phone's album or crate panel come in from their right edge, about 250 ms, fading as
// they come, and Back or their ✕ sends them back out the same way; page to page stays instant. Only translate and opacity move
// (the glass's rule, lib/glassMotion): the tablet's page narrows or widens at once and the panel moves over it. Whether the panel
// itself just opened or closed is lib/panel's to say (`panelMoved`); App runs the motion. Pure, so the tests take it as it is.
import { decelerate } from './motion';

export const PANEL_MS = 250;
/** How long a word of lib/panel's counts for the motion that follows it (the next frame or two; this is ample). */
const FRESH_MS = 1000;

/** What lib/panel said last: the panel opened by a view's open, or closed by Back or its ✕, and when; null for anything else. */
export type Moved = { how: 'open' | 'close' | null; at: number };

/** How long a panel's coming (`how` 'open') or going ('close') takes: PANEL_MS where the panel itself did so just now; none for
 *  anything else (a page gone to, a page's own part, a hand-over between places), nor with motion reduced. */
export const panelMs = (moved: Moved, how: 'open' | 'close', now: number, reduced: boolean) =>
  !reduced && moved.how === how && now - moved.at < FRESH_MS ? PANEL_MS : 0;

/** Its coming, as keyframes (the phone's layer, started once Hold shows what it holds): from its right edge, faded, to its place. */
export const SLIDE_IN: Keyframe[] = [
  { transform: 'translateX(100%)', opacity: 0 },
  { transform: 'none', opacity: 1 },
];

/** Its coming and going as a Svelte transition: t from 0 (at its right edge, faded) to 1 (in its place), back for its going. */
export function panelSlide(_node: Element, { duration }: { duration: number }) {
  return { duration, easing: decelerate, css: (t: number) => `transform: translateX(${(1 - t) * 100}%); opacity: ${t}` };
}
