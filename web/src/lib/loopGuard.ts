// One guard for every GET the page sends (lib/api fetchJson).
/** A page asking one thing over and over is held back here, before the server (the owner, 2026-11-19: "that needs to be fixed
 *  durably and robustly"). An effect that read what it wrote (F39) asked /api/artists/… thousands of times a minute, and the
 *  server, throttling the page, refused its covers too. The same GET sent more than `LOOP_MAX` times in `LOOP_MS` fails at once,
 *  said once in the console with its path. Only asks that went out count, so a path held goes again once they leave the window
 *  (the review of 1.2, M7: a busy intake's reads were held and the view stayed stale); but a held path asked again within
 *  `LOOP_GAP` stays held, for a loop never pauses. A poll (every 2 s at most) and a view's reads on the server's changes (a
 *  second apart, lib/dataChanges) never come near it. */
export const LOOP_MS = 10_000;
export const LOOP_MAX = 60;
export const LOOP_GAP = 500;
const sentAt = new Map<string, number[]>();
/** A path held, and when it was last asked. */
const heldAt = new Map<string, number>();
const loopSaid = new Set<string>();
export function loopHeld(path: string, init?: RequestInit, now = Date.now()): boolean {
  if (init?.method && init.method.toUpperCase() !== 'GET') return false;
  const times = (sentAt.get(path) ?? []).filter((t) => now - t < LOOP_MS);
  sentAt.set(path, times);
  const last = heldAt.get(path);
  if ((last !== undefined && now - last < LOOP_GAP) || times.length >= LOOP_MAX) {
    heldAt.set(path, now);
    if (!loopSaid.has(path)) {
      loopSaid.add(path);
      console.error(`Cratarr: ${path} was sent ${times.length} times in ${LOOP_MS / 1000} s, held back: a loop in the page`);
    }
    return true;
  }
  heldAt.delete(path);
  times.push(now);
  if (sentAt.size > 500) for (const [k, v] of sentAt) if (!v.length || now - v[v.length - 1] >= LOOP_MS) sentAt.delete(k);
  return false;
}
