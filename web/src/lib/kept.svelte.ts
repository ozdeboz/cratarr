// Synced to this device (decision 0038; Keep on this device until 2026-10-24): albums, playlists, crates and artists the
// Android app keeps in step offline. The app holds the list and does the work; the page switches items on and off and
// shows its progress (the Downloads page). Outside the app there is nothing to keep: `kept.known` stays false.

import { inApp, onApp, toApp } from './app.svelte';
import { toast } from './toast.svelte';

export type KeptKind = 'album' | 'playlist' | 'crate' | 'artist';

/** One kept item as the app reports it. */
export type KeptItem = {
  /** `kind:source/id` (keptKey). */
  key: string;
  kind: KeptKind;
  source: string;
  /** An album's or a crate's id, a playlist's name, an artist's name. */
  id: string;
  /** Shown in the sheet. */
  name: string;
  /** When it was kept (epoch ms): the budget shrinks the one kept longest ago first. */
  since: number;
  /** Tracks it holds now, those on the device, those that failed; those the budget leaves out. */
  total: number;
  done: number;
  failed: number;
  over_budget: number;
  /** Bytes on the device for it (a track two items hold counts for both). */
  bytes: number;
  /** done: all on the device; syncing; waiting (see `kept.waiting`); error: the item could not be read (`error`). */
  state: 'done' | 'syncing' | 'waiting' | 'error';
  error?: string | null;
};

export const kept = $state({
  items: [] as KeptItem[],
  /** The app has answered once. */
  known: false,
  /** Bytes on the device in all (kept and downloaded), and the budget (null: no limit). */
  used: 0,
  budget: null as number | null,
  /** What holds the work back: Wi-Fi, charging, no connection; null when nothing does. */
  waiting: null as 'wifi' | 'charging' | 'network' | null,
  /** The track downloading now, as "artist – name". */
  current: null as string | null,
});

export const keptKey = (kind: KeptKind, source: string, id: string) => `${kind}:${source}/${id}`;

/** Whether this item is kept on the device. */
export const isKept = (kind: KeptKind, source: string, id: string) => kept.items.some((i) => i.key === keptKey(kind, source, id));

/** Anything syncing or held back: the Downloads page's ring shows. */
export const keptBusy = () => kept.items.some((i) => i.state === 'syncing' || i.state === 'waiting');

/** Keep an item on the device, or not (`name` for the sheet). The app answers with the whole list again. */
export function setKept(kind: KeptKind, source: string, id: string, on: boolean, name: string = id) {
  if (!inApp) return;
  // At once in the page; the app's report follows.
  const key = keptKey(kind, source, id);
  if (on && !kept.items.some((i) => i.key === key)) {
    kept.items = [...kept.items, { key, kind, source, id, name, since: Date.now(), total: 0, done: 0, failed: 0, over_budget: 0, bytes: 0, state: 'syncing' }];
  } else if (!on) {
    kept.items = kept.items.filter((i) => i.key !== key);
  }
  toApp({ type: 'kept-set', kind, source, id, name, on });
}

/** Asks the app for the list and its progress again. */
export const askKept = () => inApp && toApp({ type: 'kept' });

/** Sync now (the sheet's retry), whatever the schedule says; the rules (Wi-Fi, charging) still hold. Its end is always
 *  said in a toast, whatever it came to (the owner, 2026-10-24): when the app's reports show nothing syncing any more,
 *  or, when nothing needed it (no report shows it syncing), after a moment. */
export function syncKept() {
  if (!inApp) return;
  asked = { syncing: false };
  clearTimeout(settle);
  settle = window.setTimeout(() => asked && !asked.syncing && ended(true), SETTLE_MS);
  toApp({ type: 'kept-sync' });
}

/** A sync asked for by hand, until its end is said: whether a report has shown it syncing yet. */
let asked: { syncing: boolean } | null = null;
let settle = 0;
/** How long a sync asked for may take to show in a report before nothing is taken to have needed it. */
const SETTLE_MS = 4000;
const WAITING = { wifi: 'waiting for Wi-Fi', charging: 'waiting to charge', network: 'waiting for a connection' } as const;

/** The sync's end in one line: synced (or up to date), held back and why, or what did not make it. */
function ended(nothing = false) {
  asked = null;
  clearTimeout(settle);
  const sum = (f: (i: KeptItem) => number) => kept.items.reduce((n, i) => n + f(i), 0);
  const unread = kept.items.filter((i) => i.state === 'error').length;
  const failed = sum((i) => i.failed);
  const over = sum((i) => i.over_budget);
  if (kept.waiting) toast.show({ text: `Not synced: ${WAITING[kept.waiting]}`, tone: 'warn' });
  else if (unread) toast.show({ text: `${unread} couldn't be read`, tone: 'error' });
  else if (failed) toast.show({ text: `Synced · ${failed} failed`, tone: 'warn' });
  else if (over) toast.show({ text: `Synced · ${over} over budget`, tone: 'warn' });
  else toast.show({ text: nothing ? 'Up to date' : 'Synced', tone: 'done' });
}

if (inApp) {
  onApp('kept', (m) => {
    kept.items = Array.isArray(m.items) ? (m.items as KeptItem[]) : [];
    kept.used = typeof m.used === 'number' ? m.used : 0;
    kept.budget = typeof m.budget === 'number' ? m.budget : null;
    kept.waiting = m.waiting === 'wifi' || m.waiting === 'charging' || m.waiting === 'network' ? m.waiting : null;
    kept.current = typeof m.current === 'string' ? m.current : null;
    kept.known = true;
    // A sync asked for by hand: ended once nothing syncs any more (held back by the rules counts as its end).
    if (asked) {
      if (kept.items.some((i) => i.state === 'syncing')) asked.syncing = true;
      else if (asked.syncing || kept.waiting) ended();
    }
  });
  askKept();
}
