// One request of a kind on its way at a time (0.10.1's late winds, 2026-10-30): over plain http a browser opens at most
// six connections to the server, and each request on its way holds one. The ThinkPad's state reports, sent at every
// change and every 5 s, piled up while the server was slow to answer them (7 s), and a hold's stream waited behind
// them for a free connection past its 1.5 s. Asked again while one is on its way, the request runs once more after it,
// with what was asked meanwhile merged, however often it was asked.

/** `send`, at most one at a time; asked meanwhile, once more after it with the asks merged by `merge`. */
export function oneInFlight<A>(send: (a: A) => Promise<unknown>, merge: (was: A, now: A) => A): (a: A) => Promise<void> {
  let busy = false;
  let next: { a: A } | null = null;
  const run = async (a: A): Promise<void> => {
    if (busy) {
      next = next ? { a: merge(next.a, a) } : { a };
      return;
    }
    busy = true;
    try {
      await send(a);
    } catch {
      // The caller's own to handle; the next ask still goes.
    } finally {
      busy = false;
    }
    if (!next) return;
    const { a: then } = next;
    next = null;
    return run(then);
  };
  return run;
}
