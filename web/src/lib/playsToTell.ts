// The plays not yet told (decision 0064; 0.12, decision 0085 step 3: no play lost when the page is closed meanwhile):
// reports Cratarr has not taken yet, kept on this device and sent again with the next one or when it answers again. Each
// has its id: Cratarr counts a report sent twice once. Each carries the account it was heard under and is told only while
// that account is signed in (0.13, lib/account); another's wait for it, a month at most (lib/outbox's rules). One heard with
// nobody signed in is no one's, as in the app (Plays.kt), and never kept: the next to sign in would be told it as their own.
import { ours, outlived } from './outbox';

/** After a report that moved nothing, the next waits this long. Cratarr leaves out of `done` a play it cannot count yet
 *  (a followed server away, the user data failing) and would say the same at once: going straight back is a stream of
 *  requests with no end, at a server already in trouble. A play of a track gone from such a server is never done. */
export const TELL_AGAIN_MS = 60_000;
/** The latest this many of each account's are kept. */
const KEPT = 500;

/** The latest [KEPT] of each account's, in their order: one's plays do not push out another's. */
function latestOfEach<P extends { account?: string }>(plays: P[]): P[] {
  const counted = new Map<string | undefined, number>();
  return [...plays]
    .reverse()
    .filter((p) => {
      const n = counted.get(p.account) ?? 0;
      counted.set(p.account, n + 1);
      return n < KEPT;
    })
    .reverse();
}

/** The telling: `report` sends them, `account` says who is signed in (lib/account), `keep` keeps what waits. */
export function playsToTell<P extends { id: string; at: number; account?: string }>(
  report: (plays: P[]) => Promise<{ done: string[] }>,
  account: () => string | null,
  keep: (plays: P[]) => void,
  now: () => number = Date.now,
) {
  let unsent: P[] = [];
  /** Those on their way. */
  let telling: P[] = [];
  let restUntil = 0;
  const save = () => keep(latestOfEach([...telling, ...unsent]));

  /** Tells what waits of the account signed in, unless some is on its way or the last report moved nothing just now; done
   *  once Cratarr has answered for all of it, or moved nothing. */
  function tell(): Promise<void> {
    const a = account();
    if (telling.length || a === null || now() < restUntil) return Promise.resolve();
    unsent = unsent.filter((e) => !outlived(e, a, now()));
    telling = unsent.filter((e) => ours(e, a)).slice(-KEPT);
    if (!telling.length) return Promise.resolve();
    unsent = unsent.filter((e) => !ours(e, a));
    let moved = false;
    return report(telling)
      .then(
        (r) => {
          const left = telling.filter((e) => !r.done.includes(e.id));
          moved = left.length < telling.length;
          unsent = [...left, ...unsent];
        },
        () => {
          unsent = [...telling, ...unsent];
        },
      )
      .finally(() => {
        telling = [];
        save();
        if (!moved) restUntil = now() + TELL_AGAIN_MS;
        else if (unsent.length) return tell();
      });
  }

  return {
    tell,
    /** What Cratarr has not taken yet, as it was heard: a device adds it to what the server last counted. */
    waiting: () => [...telling, ...unsent],
    /** One more heard: kept, and told. */
    add(p: P) {
      if (p.account === undefined) return;
      unsent.push(p);
      save();
      void tell();
    },
    /** What was kept here before this page: those not known yet taken in. */
    took(kept: P[]) {
      const have = new Set([...telling, ...unsent].map((e) => e.id));
      unsent = [...kept.filter((e) => !have.has(e.id) && e.account !== undefined), ...unsent];
      save();
    },
  };
}
