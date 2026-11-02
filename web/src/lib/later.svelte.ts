// The outbox (decision 0085, step 3; the owner, 2026-10-28: "the best PWA offline support possible"): a heart, a rating,
// a playlist's tracks or name, what is excluded. Each change is kept on this device first (lib/keptStore), in the order
// made, then sent (lib/api's `later`); one that does not reach Cratarr waits, and all behind it, until Cratarr answers
// again. Each carries its op id (Idempotency-Key, 0.9.15) and the time it was made (X-Cratarr-At, 0.12): one whose
// first try reached Cratarr but whose answer was lost is not made twice, and a later change made elsewhere wins. A
// change Cratarr turns down when it comes (the playlist gone meanwhile) is let go: Cratarr's state is the truth. Each
// carries the account it was made under and is sent only while that account is signed in (0.13, lib/account). The
// rules are lib/outbox's.
import { accountKey, whenAccount } from './account';
import { keep, readKept } from './keptStore';
import { headersOf, mergeOps, opsFrom, ours, outcomeOf, outlived, stale, stamped, type Op } from './outbox';
import { notReached, reach, whenReached } from './reach.svelte';

export type Later = Op;

const NAME = 'outbox';
/** 0.9.15 to 0.11's list, in localStorage: taken over once. */
const OLD_KEY = 'cratarr-later';
function oldList(): Op[] {
  try {
    return opsFrom(JSON.parse(localStorage.getItem(OLD_KEY) ?? '[]'));
  } catch {
    return [];
  }
}

export const later = $state({ list: typeof window === 'undefined' ? [] : oldList() });
/** Done once the stored outbox has been read in (until then a change waits behind it, kept in order). */
const loaded =
  typeof window === 'undefined'
    ? Promise.resolve()
    : readKept<unknown>(NAME).then(async (v) => {
        later.list = mergeOps(opsFrom(v), later.list);
        stamp();
        await save();
        try {
          localStorage.removeItem(OLD_KEY);
        } catch {
          /* nothing there */
        }
        if (later.list.length && !reach.away) void sendLater();
      });
const save = () => keep(NAME, $state.snapshot(later.list));
/** The account known: the changes carrying none take it (lib/outbox's `stamped`). */
function stamp() {
  const account = accountKey();
  if (account !== null && later.list.some((c) => c.account === undefined)) later.list = stamped(later.list, account);
}

/** A key for a change, the same for its first try and every resend. */
export const laterKey = () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`);

/** Called once what waited has been sent (lib/changes reads what changed meanwhile). */
export const whenSent = new Set<() => void>();

/** A first try or the resending at work: one at a time, so a change is never sent twice at once. */
let busy = false;
const letGo = async (key: string) => {
  later.list = later.list.filter((c) => c.key !== key);
  await save();
};

/** Keeps a change, written before it is first tried, then tries it unless others wait before it. `send` makes the
 *  request with the headers given; `unreached` tells its failure "not reached" apart. Returns what `send` returned, or
 *  null when the change waits (the caller shows its stand-in). A change turned down is let go and its error thrown. */
export async function keepAndTry<T>(op: Op, send: (headers: Record<string, string>) => Promise<T>, unreached: (e: unknown) => boolean): Promise<{ value: T } | null> {
  await loaded;
  // Another account's changes do not hold this one back; while the account is not known, any might be its own.
  const account = accountKey();
  const ahead = busy || later.list.some((c) => account === null || ours(c, account));
  later.list = [...later.list, account === null ? op : { ...op, account }];
  await save();
  if (ahead) {
    void sendLater();
    return null;
  }
  busy = true;
  try {
    const value = await send(headersOf(op));
    await letGo(op.key);
    return { value };
  } catch (e) {
    if (unreached(e)) return null;
    await letGo(op.key);
    throw e;
  } finally {
    busy = false;
    if (later.list.length && !reach.away) void sendLater();
  }
}

/** Sends what waits of the account signed in, in order, while Cratarr answers; stops at the first that does not reach
 *  it. Another account's changes wait for it (a month at most). */
export async function sendLater() {
  await loaded;
  const account = accountKey();
  if (busy || !later.list.length || account === null) return;
  busy = true;
  let sent = 0;
  try {
    const kept = later.list.filter((c) => !outlived(c, account, Date.now()));
    if (kept.length < later.list.length) {
      console.warn('Changes made under another account were let go, never taken', later.list.length - kept.length);
      later.list = kept;
      await save();
    }
    let c: Op | undefined;
    while ((c = later.list.find((x) => ours(x, account)))) {
      if (stale(c, Date.now())) {
        console.warn('A change made away was let go, never taken', c.method, c.path);
        await letGo(c.key);
        continue;
      }
      let r: Response | null = null;
      try {
        r = await fetch(c.path, { method: c.method, headers: headersOf(c), body: c.body });
      } catch {
        /* not reached */
      }
      const outcome = outcomeOf(r?.status ?? null, !!r?.headers.has('Retry-After'), notReached);
      if (outcome === 'wait') break;
      if (outcome === 'again') {
        await new Promise((ok) => setTimeout(ok, 1000));
        continue;
      }
      if (outcome === 'refused') console.warn('A change made away was turned down', c.method, c.path, r?.status);
      await letGo(c.key);
      sent++;
    }
  } finally {
    busy = false;
  }
  if (sent && !later.list.some((c) => ours(c, account))) for (const f of whenSent) f();
}

whenReached.add(() => void sendLater());
whenAccount.add(async () => {
  await loaded;
  stamp();
  await save();
  void sendLater();
});
