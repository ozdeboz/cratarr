// A poll that never stops (RC01: 0.16.2's status poll awaited each answer before it asked again, so one request that never
// ended stopped it for good, and the desktop app said "unreachable" for hours while Cratarr was well). Asked on a timer, never
// in a chain that waits; the answers taken in the order asked, one older than one already taken dropped. Pure of the page.

/** What one ask came to: its answer, or why it failed. */
export type Polled<T> = { ok: true; value: T } | { ok: false; error: unknown };

/** Asks `ask` at once and every `ms`, whatever became of the asks before (each request has its own limit: lib/api's `watch`);
 *  `take` hears each answer or failure, never one older than one it has taken, and nothing once stopped. Returns its stop. */
export function pollEvery<T>(ms: number, ask: () => Promise<T>, take: (r: Polled<T>) => void): () => void {
  let asked = 0;
  let taken = 0;
  let stopped = false;
  const one = () => {
    const n = ++asked;
    const told = (r: Polled<T>) => {
      if (stopped || n < taken) return;
      taken = n;
      take(r);
    };
    let p: Promise<T>;
    try {
      p = ask();
    } catch (error) {
      p = Promise.reject(error);
    }
    p.then(
      (value) => told({ ok: true, value }),
      (error) => told({ ok: false, error }),
    );
  };
  one();
  const t = setInterval(one, ms);
  return () => {
    stopped = true;
    clearInterval(t);
  };
}
