// The library on this device, and the one place that says where reads come from (lib/deviceIndexRule's `readsFrom`). No page asks
// whether it is offline: reads go through lib/api's `read`, and whatever must read again when they move is told (`whenReadsMove`).
import { lookedAgain, notReached, readsFrom, answered, type ReadsFrom, type Settle } from './deviceIndexRule';
import { lookForCratarr, reach, whenAway, whenReached } from './reach.svelte';

export { readsFrom };

let settle: Settle = answered;
let looking: ReturnType<typeof setTimeout> | null = null;

/** Where reads come from now. */
export const readsNow = (): ReadsFrom => readsFrom({ unreachable: settle.failed === 2, noNetwork: reach.noNetwork });

/** Called when reads move, either way: what a page showed came from the other place. */
export const whenReadsMove = new Set<() => void>();

let wereFrom = readsNow().from;
function moved() {
  const now = readsNow().from;
  if (now === wereFrom) return;
  wereFrom = now;
  for (const f of whenReadsMove) f();
}

function secondLook() {
  looking = null;
  void lookForCratarr().then((there) => {
    settle = lookedAgain(settle, there);
    moved();
  });
}

function away() {
  settle = notReached(settle, Date.now());
  if (settle.lookAt !== null && looking === null) looking = setTimeout(secondLook, Math.max(0, settle.lookAt - Date.now()));
  moved();
}

whenAway.add(away);
whenReached.add(() => {
  settle = answered;
  if (looking !== null) clearTimeout(looking);
  looking = null;
  moved();
});
if (reach.away) away();
