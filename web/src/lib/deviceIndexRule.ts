// Where a page's reads come from, pure, the time handed in (docs/architecture.md's Reads door; the owner's North Star, 2026-11-07:
// no page ever asks whether it is offline). One decision in one place, and everything that could make a device read itself is
// an input to it: Cratarr out of reach today, no mobile data in 0.17, a switch later.
//
// Out of reach settles first (audit/FINAMP-OFFLINE.md's A1): a request that failed is looked at again seconds later, and only two
// failed looks move the reads, or a blip would move every page. A device saying it has no network needs no second look. The player
// goes by none of this: it holds at the first failed fetch (lib/player/holdOn, D06), as stopping is not moving a page.

/** What the decision goes by. */
export type ReadInputs = {
  /** Cratarr was looked for twice, seconds apart, and did not answer. */
  unreachable: boolean;
  /** The device says it has no network at all: the browser's word, or the app's (Reach.kt is an input here, and decides nothing). */
  noNetwork: boolean;
};

/** Where reads come from now, and why. Whether the device has an answer is the read's own to say (lib/api's `read`). */
export type ReadsFrom = { from: 'server' | 'device'; why: 'answers' | 'unreachable' | 'no-network' };

export function readsFrom(i: ReadInputs): ReadsFrom {
  if (i.noNetwork) return { from: 'device', why: 'no-network' };
  return i.unreachable ? { from: 'device', why: 'unreachable' } : { from: 'server', why: 'answers' };
}

/** What This device says of the library kept here: fetching, asking, waiting for Wi-Fi, the connection gone again and again, or the
 *  two stopped by their versions. */
export type IndexShows = 'idle' | 'fetching' | 'asks' | 'wifi' | 'paused' | 'stopped';

/** A part of what a device keeps, as the row goes by it: what the rule said of it, in a browser's own look or the app's. */
export type PartFacts = { whole: boolean; stopped: boolean; paused: boolean; asks: boolean };

/** What the row shows. A question stands only while nobody has answered it: the app's word on a part can be older than the answer. */
export function indexShows(parts: PartFacts[], mobile: 'unasked' | 'yes' | 'no', metered: boolean): IndexShows {
  if (parts.some((p) => p.stopped)) return 'stopped';
  if (mobile === 'unasked' && parts.some((p) => p.asks)) return 'asks';
  if (mobile === 'no' && metered && parts.some((p) => p.whole)) return 'wifi';
  return parts.some((p) => p.paused) ? 'paused' : 'idle';
}

/** Its row's value: the date it last stood where Cratarr does, unless something stands in the way. */
export function indexWord(shows: IndexShows, at: number | null, date: (ms: number) => string): string {
  if (shows === 'stopped') return 'Differs from Cratarr';
  if (shows === 'wifi' || shows === 'asks') return 'Waiting for Wi-Fi';
  if (shows === 'paused') return 'Paused';
  if (at !== null) return date(at);
  return shows === 'fetching' ? 'Fetching' : 'Not yet';
}

/** How long after a failed reach Cratarr is looked for again. Finamp's number: nothing here has measured it. */
export const LOOK_AGAIN_MS = 7_000;

/** Cratarr's reach as settled: how many looks failed in a row (two settle it), and when the second is due. */
export type Settle = { failed: 0 | 1 | 2; lookAt: number | null };

/** It answered. */
export const answered: Settle = { failed: 0, lookAt: null };

/** A request did not reach it at `now`: the first starts the wait for the second look, and the rest change nothing. */
export const notReached = (s: Settle, now: number): Settle => (s.failed === 0 ? { failed: 1, lookAt: now + LOOK_AGAIN_MS } : s);

/** The second look's answer. One that comes after Cratarr answered some other way says nothing. */
export const lookedAgain = (s: Settle, there: boolean): Settle => (s.failed !== 1 ? s : there ? answered : { failed: 2, lookAt: null });
