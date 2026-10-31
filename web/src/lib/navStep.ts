// Navigation as one step (0.10; the owner, 2026-10-30: a search result opening a full-screen panel showed the plain
// Library a moment before the panel, and back brought the results back only after a while): what a tap leaves (the
// search's words or its scope, the player over the page, a panel's layer) is let go when where it goes SHOWS (App's
// page and the panel as lib/Hold shows them), not at the tap; where it never shows (gone back before it did, nothing
// moved at all), nothing is lost. Pure: lib/navStep.svelte.ts runs it.

/** What a step leaves, let go when it arrives. `search`: its words emptied, or only its scope back to the page's
 *  (found deep: the place opens narrowed to the words, decision 0072); `restore`: back to a search (lib/globalSearch),
 *  its words and scope as they were, which wins over emptying them; `player`: the player over the page closes;
 *  `layer`: a closed panel's layer stays over the page until the page gone to shows (a breadcrumb's); `quick`: the
 *  crate being made (lib/quickCrates) is emptied, saved as the crate that opens (its words stay until that shows). */
export type Leave = {
  search?: 'clear' | 'page';
  restore?: { q: string; scope: 'page' | 'everywhere' };
  player?: boolean;
  layer?: boolean;
  quick?: boolean;
};
/** Where the page is, as asked (the address, `hash` its page's part) or as shown (lib/Hold): its page and what its
 *  panel views. */
export type Where = { page: string; viewed: string | null; hash?: string };
/** A step on its way: what it leaves, where it was asked from, where it goes (once it moved), since when; `until`: the
 *  address it is on its way to, when that takes more than one move (a panel closed, then the page changed). */
export type Pending = { leave: Leave; from: Where | null; to: Where | null; at: number; until?: string };

/** How long a step waits for anything to move before it lets go of nothing (a tap that opened a menu instead). */
export const MOVE_MS = 20_000;

const same = (a: Where | null, b: Where | null) => !!a && !!b && a.page === b.page && a.viewed === b.viewed;

/** Two steps' leaves as one: emptying the words over keeping them in the page's scope; a restore over both. */
export function merge(a: Leave, b: Leave): Leave {
  const out: Leave = { ...a, ...b };
  const search = a.search === 'clear' || b.search === 'clear' ? 'clear' : (a.search ?? b.search);
  if (search) out.search = search;
  if (b.restore ?? a.restore) out.restore = b.restore ?? a.restore;
  if (out.restore) delete out.search;
  if (a.player || b.player) out.player = true;
  if (a.layer || b.layer) out.layer = true;
  if (a.quick || b.quick) out.quick = true;
  return out;
}

/** A step begun `here` (`moved`: the move is done already, a page changed or back taken), with one on its way merged. */
export function arm(p: Pending | null, leave: Leave, here: Where, now: number, moved = false, until?: string): Pending {
  const out: Pending = !p
    ? { leave, from: moved ? null : here, to: moved ? here : null, at: now }
    : { leave: merge(p.leave, leave), from: p.from, to: moved ? here : p.to, at: p.to || moved ? p.at : now, ...(p.until ? { until: p.until } : {}) };
  if (until) out.until = until;
  return out;
}

/** Where a step stands with the page asked and shown: let go (`apply`) once all that is asked shows; dropped (both
 *  null) when it went back to where it began before showing, or nothing moved in MOVE_MS; else still on its way. */
export function step(p: Pending, asked: Where, shown: Where, now: number): { pending: Pending | null; apply: Leave | null } {
  let to = p.to;
  if (!same(asked, p.from) && !same(asked, to)) to = asked;
  else if (to && same(asked, p.from)) return { pending: null, apply: null };
  if (!to) return now - p.at > MOVE_MS ? { pending: null, apply: null } : { pending: p, apply: null };
  const there = !p.until || (asked.hash ?? '').startsWith(p.until);
  const settled = there && shown.page === asked.page && (asked.viewed === null || shown.viewed === asked.viewed);
  return settled ? { pending: null, apply: p.leave } : { pending: { ...p, to }, apply: null };
}
