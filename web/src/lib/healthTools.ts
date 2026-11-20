// What can be done for a Health item (F28.2): one when-table, read by Health's panel (a row opened in place) and by a record's
// health sheet (lib/RecordProblemSheet) alike, in today's order. Pure, so the tests take it as it is.
import type { HealthItem, RecordFinding } from './api';

/** A finding's level, as Health and a record's sheet say it. */
export const LEVEL = { critical: 'Critical', warning: 'Warning', info: 'To know' } as const;

/** The log events behind an item grouped from them (health.rs `grouped`): their area, event and grouping field. */
export const EVENTS: Record<string, { area: string; what: string; by: string }> = {
  'server:slow': { area: 'server', what: 'slow response', by: 'path' },
  'server:runaway': { area: 'server', what: 'client requests too often', by: 'client' },
  'sign-in:refused': { area: 'signin', what: 'request refused', by: 'path' },
  'sign-in:throttled': { area: 'signin', what: 'sign-in throttled', by: 'client' },
};

/** The findings Renumber fixes (it writes disc and track numbers): track numbers, and disc numbers. */
export const RENUMBERS = new Set(['library:track-numbers', 'library:discs']);

/** What a missing-tracks item is asked for by: its record where it is Cratarr's own, else its release group. */
export const fullOf = (x: HealthItem) => (x.album?.[0] === 'cratarr' ? x.album[1] : (x.release_group ?? null));

export type ToolKind =
  | 'playlistMissing'
  | 'reconnect'
  | 'gone'
  | 'heldGone'
  | 'incoming'
  | 'renumber'
  | 'references'
  | 'events'
  | 'missingTracks'
  | 'askFull'
  | 'files'
  | 'fileLine'
  | 'fixes'
  | 'betterCopy'
  | 'changeCover'
  | 'goTo';

/** Who asks, and where: an administrator; tracks looked for here (`finds`); Renumber offered; the finding's fixes reviewable;
 *  `here`, the record a sheet is over (no Go to it: F28.3, no U-turn). */
export type ToolsContext = { admin?: boolean; finds?: boolean; renumber?: boolean; fixable?: boolean; here?: string | null };

const fileLine = (x: HealthItem) => !x.album && !x.playlist && !x.track && !x.files?.length && !!x.line;
const cratarr = (x: HealthItem) => x.album?.[0] === 'cratarr';

/** The when-table: each tool, and when it shows, in the order the row shows them. */
const WHEN: [ToolKind, (problem: string, x: HealthItem, c: ToolsContext) => boolean][] = [
  ['playlistMissing', (_, x) => !!x.playlist],
  ['reconnect', (p, x) => p === 'library:missing' && !!x.track],
  // Gone (LS05): a missing track let go, an administrator's word; a held library's walk let through, likewise.
  ['gone', (p, x, c) => p === 'library:missing' && !!x.track && !!c.admin],
  ['heldGone', (p, x, c) => p === 'library:held' && !!x.folder && !!c.admin],
  ['incoming', (p, x) => p.startsWith('incoming:') && !!x.album],
  ['renumber', (p, x, c) => RENUMBERS.has(p) && cratarr(x) && !!c.renumber],
  ['references', (p, x) => p === 'library:references' && cratarr(x)],
  ['events', (p) => !!EVENTS[p]],
  ['missingTracks', (_, x) => !!x.missing?.length],
  ['askFull', (_, x) => !!x.missing?.length && !!fullOf(x)],
  ['files', (_, x) => !!x.files?.length],
  ['fileLine', (_, x) => fileLine(x)],
  ['fixes', (_, x, c) => fileLine(x) && !!c.fixable],
  ['betterCopy', (_, x, c) => !!x.track && !!x.album && !!c.finds],
  ['changeCover', (p, x, c) => p === 'library:cover' && cratarr(x) && !!c.admin],
  ['goTo', (_, x, c) => (!!x.album || !!x.playlist) && !(c.here && x.album?.[1] === c.here)],
];

/** Missing music's tracks among `items`, each one "All gone" lets go (LS05): only tracks, never a parked library or a held
 *  retired one, which have no track of their own. */
export const goneTracks = (items: readonly HealthItem[]) => items.flatMap((x) => (x.track ? [x.track] : []));
/** Whether a finding offers "All gone" (LS05): Missing music with tracks to let go, to an administrator. */
export const allGone = (problem: string, items: readonly HealthItem[], admin: boolean) => problem === 'library:missing' && admin && goneTracks(items).length > 0;

/** What can be done for `item` of `problem` (a finding's key, a files kind's `files-<kind>`), in today's order. */
export function itemTools(problem: string, item: HealthItem, ctx: ToolsContext = {}): ToolKind[] {
  return WHEN.filter(([, when]) => when(problem, item, ctx)).map(([kind]) => kind);
}

/** The finding a record's sheet shows (F28.5, F28.6): the one the tapped line's key names, as the record's health reads now;
 *  none where it is gone (fixed, the record merged or gone), which closes the sheet. `keyOf`: a finding's line key. */
export function sheetFor<F extends Pick<RecordFinding, 'key'>>(findings: readonly F[] | null | undefined, key: string | null, keyOf: (f: F) => string): F | null {
  if (!key || !findings) return null;
  return findings.find((f) => keyOf(f) === key) ?? null;
}
