// The Library's Classical (0.10; the owner, 2026-10-30): chosen as its kind, it opens on its composers (GET
// /api/classical/composers), a record under each of its composers; a composer opens their records, the Library's own
// list narrowed to them. The composer rides in the address (`#/library?composer=<key>`, kept as the panel opens over
// it, lib/panel), so back returns to the composers as any step does.
import { getClassicalComposer, getClassicalComposers } from './api';
import { byComposer, type Composer } from './recordKindRules';

export const classical = $state({
  composers: null as Composer[] | null,
  error: null as string | null,
  /** The composer whose records show, and those records' album ids. */
  composer: null as { key: string; name: string; records: Set<string> } | null,
});

let asked = false;
/** The composers, read once (again with `fresh`). */
export function loadComposers(fresh = false) {
  if (asked && !fresh) return;
  asked = true;
  getClassicalComposers().then(
    (l) => ((classical.composers = [...(l ?? [])].sort(byComposer)), (classical.error = null)),
    (e) => (classical.error = e instanceof Error ? e.message : String(e)),
  );
}

/** The composer the address names, if any. */
export const composerInAddress = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('composer');

/** A composer opened: a step of its own in the address. */
export function openComposer(c: Composer) {
  const [h, rest] = location.hash.split('?');
  const q = new URLSearchParams(rest ?? '');
  q.set('composer', c.key);
  location.hash = `${h}?${q}`;
}

let shownKey: string | null = null;
/** The composer's records for the address's composer (none: the list of composers). */
export function followAddress() {
  const key = composerInAddress();
  if (key === shownKey) return;
  shownKey = key;
  if (!key) return void (classical.composer = null);
  const known = classical.composers?.find((c) => c.key === key);
  getClassicalComposer(key).then(
    (c) => shownKey === key && (classical.composer = { key, name: c.name || known?.name || '', records: new Set(c.records) }),
    (e) => shownKey === key && (classical.error = e instanceof Error ? e.message : String(e)),
  );
}
if (typeof window !== 'undefined') addEventListener('hashchange', followAddress);
