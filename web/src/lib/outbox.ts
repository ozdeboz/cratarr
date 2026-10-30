// The outbox's rules (decision 0085, step 3; 0087): every change the page makes to Cratarr is kept on this device
// first, in the order made, and sent in that order. Each carries its op id, sent as Idempotency-Key, so a change sent
// twice (its answer lost) is made once; and the time it was made, sent as X-Cratarr-At, so a later change made
// elsewhere wins (favourites and ratings, last change by time). The keeping and the sending are lib/later's.

/** One change waiting: its request as it is sent, its op id (`key`) and when it was made (`at`, epoch ms). */
export type Op = { key: string; method: string; path: string; body: string | null; type: string | null; at: number };

/** A week waiting (each try turned away as though out of reach): let go, not to hold the rest back for ever. */
export const STALE_MS = 7 * 86_400_000;
export const stale = (op: Op, now: number) => now - op.at > STALE_MS;

/** What an answer means for the change at the head of the outbox. `sent`: taken, let go. `refused`: turned down (the
 *  playlist gone meanwhile), let go too: Cratarr's state is the truth. `wait`: not reached, not signed in, or Cratarr
 *  not well: it and all behind it wait. `again`: its first try still at work there: once more in a moment. */
export type Outcome = 'sent' | 'refused' | 'wait' | 'again';

/** `status` null: the request did not reach Cratarr. `notReached`: lib/reach's rule for statuses that mean the same. */
export function outcomeOf(status: number | null, retryAfter: boolean, notReached: (status: number) => boolean): Outcome {
  if (status === null) return 'wait';
  if (status === 409 && retryAfter) return 'again';
  if (notReached(status) || status === 401 || status >= 500) return 'wait';
  return status >= 200 && status < 300 ? 'sent' : 'refused';
}

/** The headers a change is sent with, its first try and every resend alike. */
export function headersOf(op: Op): Record<string, string> {
  return { 'Idempotency-Key': op.key, 'X-Cratarr-At': String(op.at), ...(op.type ? { 'Content-Type': op.type } : {}) };
}

/** Two copies of the outbox as one (this page's, and the store's read a moment later): each change once, by its op
 *  id, in the order made (by `at`; the first copy's order where two were made in the same millisecond). */
export function mergeOps(first: readonly Op[], second: readonly Op[]): Op[] {
  const seen = new Set<string>();
  const all: { op: Op; n: number }[] = [];
  for (const op of [...first, ...second]) {
    if (seen.has(op.key)) continue;
    seen.add(op.key);
    all.push({ op, n: all.length });
  }
  return all.sort((a, b) => a.op.at - b.op.at || a.n - b.n).map((x) => x.op);
}

/** A list read back from storage, as far as it is one: anything else is no list. */
export function opsFrom(v: unknown): Op[] {
  if (!Array.isArray(v)) return [];
  return v.filter(
    (o): o is Op =>
      !!o &&
      typeof o === 'object' &&
      typeof (o as Op).key === 'string' &&
      typeof (o as Op).method === 'string' &&
      typeof (o as Op).path === 'string' &&
      typeof (o as Op).at === 'number',
  );
}
