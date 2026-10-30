// Navigation as one step, run (lib/navStep): a step begun at a tap is let go of when App's page and the panel show
// what is asked (App keeps `navPlace`; the panel's `shown` is lib/Hold's). Who lets go of what is said where it lives:
// the search's words (lib/globalSearch), the player over the page and a closed panel's layer (App).
import { untrack } from 'svelte';
import { arm, MOVE_MS, step, type Leave, type Pending, type Where } from './navStep';
import { panel } from './panel.svelte';

/** App's page as asked and as shown (lib/Hold), App's to keep; `layerHeld`: a closed panel's layer stays over the page
 *  until the page gone to shows. */
export const navPlace = $state({ page: '', shownPage: '', layerHeld: false });

const letGo: ((l: Leave) => void)[] = [];
/** What lets go of its part of a step, once it arrives. */
export const onArrival = (f: (l: Leave) => void) => void letGo.push(f);

let pending: Pending | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
const asked = (): Where => ({ page: navPlace.page, viewed: panel.viewed, hash: location.hash.split('?')[0] || '#/' });
const shown = (): Where => ({ page: navPlace.shownPage, viewed: panel.shown });

function run() {
  if (!pending) return;
  const r = step(pending, asked(), shown(), Date.now());
  pending = r.pending;
  navPlace.layerHeld = !!pending?.leave.layer;
  if (!pending) clearTimeout(timer);
  if (r.apply) for (const f of letGo) f(r.apply);
}

/** A step: what it leaves let go of when where it goes shows. Called before the move (a tap's), or with `moved` after
 *  it (a page changed, back taken); `until`: the address it goes to, when it takes more than one move. */
export function leaveOnArrival(leave: Leave, moved = false, until?: string) {
  pending = arm(pending, leave, untrack(asked), Date.now(), moved, until);
  navPlace.layerHeld = !!pending.leave.layer;
  clearTimeout(timer);
  timer = setTimeout(() => untrack(run), MOVE_MS + 50);
  if (moved) untrack(run);
}
/** How soon a step that moved nothing (what it opens shows already: an album viewed under the player) lets the player
 *  go all the same. */
const STILL_MS = 150;
/** From the player over the page: it closes once what was opened shows (it covered it meanwhile), or at once where that
 *  shows already. */
export function leavePlayer() {
  leaveOnArrival({ player: true });
  setTimeout(() => {
    if (!pending || pending.to || !pending.leave.player) return;
    pending = { ...pending, leave: { ...pending.leave, player: false } };
    for (const f of letGo) f({ player: true });
  }, STILL_MS);
}

if (typeof window !== 'undefined') {
  $effect.root(() => {
    $effect(() => {
      void [navPlace.page, navPlace.shownPage, panel.viewed, panel.shown];
      untrack(run);
    });
  });
}
