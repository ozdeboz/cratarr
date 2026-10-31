// Scrolled to what a link leads to (the owner, 2026-10-30, docs/plans.md 0.13): a track the panel was opened for, the
// item beside it in the page's list; and every scroll the page makes to a thing stops with it whole in sight, clear of
// the bar, the mini-player, a sticky action bar and the floating glass. One rule for that: each box that scrolls keeps
// those heights as its scroll-padding (app.css `html`, PlaylistView's `.scroll`), from the same properties that room
// is made with, and a thing is brought into sight with scrollIntoView, which keeps to it. No place counts its own
// offset.
import { reducedMotion } from './motion';

/** Where a list that makes only what is near (lib/nearBlocks) would make `key`: a stand-in to bring near first. */
type Near = (key: string) => Element | null;
const nears = new Set<Near>();

/** A list that makes only what is near says where it would make a thing, for as long as it shows. */
export function nearFor(f: Near): () => void {
  nears.add(f);
  return () => nears.delete(f);
}

/**
 * Brings `find()` into sight in the box that scrolls it, `block` as scrollIntoView's (nearest: no move if it is in
 * sight already). Its list may still be coming in (an address opened, the library read), so it looks again each frame
 * for `wait` ms; a thing a list has not made yet (`key`, lib/nearBlocks) is brought near first, then into sight.
 * Gives a stop, for the effect that asked.
 */
export function reveal(find: () => Element | null | undefined, { block = 'nearest', key, wait = 4000 }: { block?: ScrollLogicalPosition; key?: string; wait?: number } = {}): () => void {
  const until = performance.now() + wait;
  let moved = false;
  let frame = 0;
  const look = () => {
    const el = find();
    if (el) {
      // Smooth only where it was there to begin with: one made on the way lands at once, as the list it is in did.
      el.scrollIntoView({ block, behavior: moved || reducedMotion() ? 'auto' : 'smooth' });
      return;
    }
    const stand = key ? [...nears].map((f) => f(key)).find(Boolean) : null;
    if (stand) {
      stand.scrollIntoView({ block: 'center', behavior: 'auto' });
      moved = true;
    }
    if (performance.now() < until) frame = requestAnimationFrame(look);
  };
  frame = requestAnimationFrame(look);
  return () => cancelAnimationFrame(frame);
}

/** A thing in a list, by what it is (`data-subject`: a panel subject, lib/panel), for `reveal`. */
export const subjectIn = (root: ParentNode | null | undefined, subject: string) => root?.querySelector(`[data-subject="${CSS.escape(subject)}"]`) ?? null;
