// Changes made while Cratarr is out of reach (the owner, 2026-10-28: "the best PWA offline support possible"): a heart,
// a rating, a playlist's tracks or name, what is excluded, kept on this device in the order made and sent when Cratarr
// answers again (lib/api's `later`). Each carries the key it was first tried with (Idempotency-Key, 0.9.15): one
// whose first try reached Cratarr but whose answer was lost is not made twice. A change Cratarr turns down when it comes
// (the playlist gone meanwhile) is let go: Cratarr's state is the truth.
import { notReached, reach, whenReached } from './reach.svelte';

export type Later = { key: string; method: string; path: string; body: string | null; type: string | null; at: number };

const KEY = 'cratarr-later';
function load(): Later[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}
export const later = $state({ list: load() });
function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(later.list));
  } catch {
    // A private window, or storage refused: kept while the page is open.
  }
}

/** A key for a change, the same for its first try and every replay. */
export const laterKey = () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`);

/** Keeps a change for when Cratarr answers again. */
export function keepForLater(c: Omit<Later, 'at'>) {
  later.list = [...later.list, { ...c, at: Date.now() }];
  save();
}

const STALE_MS = 7 * 86_400_000;
let sending = false;
/** Sends what waits, in order, while Cratarr answers; stops at the first that does not reach it. */
export async function sendLater() {
  if (sending || !later.list.length) return;
  sending = true;
  try {
    while (later.list.length) {
      const c = later.list[0];
      // A week waiting (each try turned away as though out of reach): let go, not to hold the rest back for ever.
      if (Date.now() - c.at > STALE_MS) {
        console.warn('A change made away was let go, never taken', c.method, c.path);
        later.list = later.list.slice(1);
        save();
        continue;
      }
      let r: Response;
      try {
        r = await fetch(c.path, {
          method: c.method,
          headers: { 'Idempotency-Key': c.key, ...(c.type ? { 'Content-Type': c.type } : {}) },
          body: c.body,
        });
      } catch {
        break;
      }
      // The first try still at work there: once more in a moment.
      if (r.status === 409 && r.headers.has('Retry-After')) {
        await new Promise((ok) => setTimeout(ok, 1000));
        continue;
      }
      // Not reached, not signed in, or Cratarr not well: it waits on.
      if (notReached(r.status) || r.status === 401 || r.status >= 500) break;
      if (!r.ok) console.warn('A change made away was turned down', c.method, c.path, r.status);
      later.list = later.list.slice(1);
      save();
    }
  } finally {
    sending = false;
  }
}

whenReached.add(() => void sendLater());
if (typeof window !== 'undefined' && later.list.length && !reach.away) void sendLater();
