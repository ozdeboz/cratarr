// One guard for every GET the page sends (lib/api fetchJson).
/** A page asking one thing over and over is held back here, before the server (the owner, 2026-11-19: "that needs to be fixed
 *  durably and robustly"). An effect that read what it wrote (F39) asked /api/artists/… thousands of times a minute, and the
 *  server, throttling the page, refused its covers too. The same GET more than `LOOP_MAX` times in `LOOP_MS` fails at once for the
 *  rest of that window, said once in the console with its path; a poll (every 2 s at most) never comes near it. */
export const LOOP_MS = 10_000;
export const LOOP_MAX = 20;
const askedAt = new Map<string, number[]>();
const loopSaid = new Set<string>();
export function loopHeld(path: string, init?: RequestInit, now = Date.now()): boolean {
  if (init?.method && init.method.toUpperCase() !== 'GET') return false;
  const times = (askedAt.get(path) ?? []).filter((t) => now - t < LOOP_MS);
  times.push(now);
  askedAt.set(path, times);
  if (askedAt.size > 500) for (const [k, v] of askedAt) if (now - v[v.length - 1] >= LOOP_MS) askedAt.delete(k);
  if (times.length <= LOOP_MAX) return false;
  if (!loopSaid.has(path)) {
    loopSaid.add(path);
    console.error(`Cratarr: ${path} was asked ${times.length} times in ${LOOP_MS / 1000} s, held back: a loop in the page`);
  }
  return true;
}
