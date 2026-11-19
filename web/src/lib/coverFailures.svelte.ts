// Covers that failed to load, kept so every place that shows one shows the stand-in at once. Only a real "no cover" is kept:
// a record with none answers 404 at every size. Any other failure says nothing of the record: a server error, a cover refused
// under load, or one asked while Cratarr was out of reach (0.16.0 on the tablet, 2026-11-08: every cover asked with Wi-Fi off
// stayed a stand-in once it was back; the review of 1.2, M4: under load, stand-ins on owned albums until a reload). Such a
// failure shows the stand-in for a pause and is then forgotten, so whoever shows it asks again; the same address failing
// again waits longer, never for good. And what failed while the live channel was down, or in the moments before it was seen
// to drop, is forgotten when it comes back.
import { stream } from './events.svelte';

/** How long before the channel is seen to drop a failure may still be the network's: a request in flight fails first. */
export const GRACE_MS = 30_000;
/** The one answer that says a record has no cover. */
export const NO_COVER = 404;
/** The pauses before an address that failed for another reason is asked again: its first failure, its second, …, the last
 *  repeated. */
export const AGAIN_AFTER = [5_000, 30_000, 120_000, 300_000];

export type Failures = {
  /** The covers that failed, by their address: read it to show the stand-in. */
  readonly failed: Record<string, true>;
  /** `src` failed. `status`: what it answered (NO_COVER, another status, or null where the browser does not say); left
   *  out, the browser's resource timing is asked (`answered`). */
  note(src: string, status?: number | null): void;
};

type Kept = { failed: Record<string, true>; at: Map<string, number>; forget(src: string): void };
const all: Kept[] = [];
let downSince: number | null = null;

/** A set of failures of its own, forgotten with every other's when the channel comes back. */
export function failures(): Failures {
  const failed = $state<Record<string, true>>({});
  const at = new Map<string, number>();
  /** Addresses that answered "no cover": kept until the channel comes back. */
  const none = new Set<string>();
  /** How often each address failed for another reason, for its next pause. */
  const tries = new Map<string, number>();
  const again = new Map<string, ReturnType<typeof setTimeout>>();
  const forget = (src: string) => {
    clearTimeout(again.get(src));
    again.delete(src);
    none.delete(src);
    delete failed[src];
    at.delete(src);
  };
  all.push({ failed, at, forget });
  return {
    failed,
    note(src: string, status = answered(src)) {
      if (status === NO_COVER) {
        clearTimeout(again.get(src));
        again.delete(src);
        none.add(src);
      } else if (failed[src]) {
        // Every place showing it fails at once: one pause, from the first.
        return;
      } else {
        const n = tries.get(src) ?? 0;
        tries.set(src, n + 1);
        again.set(
          src,
          setTimeout(() => {
            again.delete(src);
            if (!none.has(src)) forget(src);
          }, AGAIN_AFTER[Math.min(n, AGAIN_AFTER.length - 1)]),
        );
      }
      failed[src] = true;
      at.set(src, Date.now());
    },
  };
}

/** The channel came back after being down since `since`: what failed from a little before then on is asked again. */
export function forgetSince(since: number) {
  const from = since - GRACE_MS;
  for (const { at, forget } of all) {
    for (const [src, when] of at) if (when >= from) forget(src);
  }
}

/** The channel as it is now: `connected` false notes when it went, true forgets what failed since. */
export function channel(connected: boolean, now = Date.now()) {
  if (!connected) {
    downSince ??= now;
  } else if (downSince !== null) {
    forgetSince(downSince);
    downSince = null;
  }
}

// What a failed address answered: an image's error says nothing of why, the browser's resource timing does (Chromium:
// `responseStatus`, the WebView and the desktop app included). Only the last answer of each address that failed is kept; a
// success, or an answer with no status (a network failure, a browser that does not say), forgets it.
const answers = new Map<string, number>();
/** Keeps the failed answers among `entries` in `seen`, by their address; forgets the address on any other. */
export function foldAnswers(seen: Map<string, number>, entries: Iterable<PerformanceEntry>) {
  for (const e of entries) {
    const status = (e as PerformanceResourceTiming).responseStatus;
    seen.delete(e.name);
    if (status >= 400) seen.set(e.name, status);
  }
  // A page that runs for days: the oldest go first.
  for (const k of seen.keys()) {
    if (seen.size <= 2000) break;
    seen.delete(k);
  }
}
let timing: PerformanceObserver | null = null;
try {
  if (typeof window !== 'undefined' && PerformanceObserver.supportedEntryTypes?.includes('resource')) {
    timing = new PerformanceObserver((list) => foldAnswers(answers, list.getEntries()));
    timing.observe({ type: 'resource', buffered: true });
  }
} catch {
  timing = null;
}
/** What `src` answered when it last failed, or null where the browser does not say. */
export function answered(src: string): number | null {
  if (!timing) return null;
  // Its entry is queued when the answer is in, before the image's error is told; the observer's call may come later.
  foldAnswers(answers, timing.takeRecords());
  try {
    return answers.get(new URL(src, location.href).href) ?? null;
  } catch {
    return null;
  }
}

$effect.root(() => {
  $effect(() => channel(stream.connected));
});
