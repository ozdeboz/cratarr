// A label's page (W11, C56): its records by year, newest first, what the house has apart from what it hasn't (F10), with their
// catalogue numbers and requests, read from MusicBrainz's and Discogs' lists a hundred at a time (OQ10 (a): "More" reads on as
// the list's end comes into view). In the panel as `label:la-<n>`; a record's About names its labels as chips that open it, by
// its row or found or made by its reference first (F9). A row opens its record, made one at open from what its source knows it
// by and tied to the label (lib/openRecord). Pure.
import type { ListRead, RequestMark } from './api';
import { editionsMark } from './editions';
import type { Openable } from './openRecord';

/** A label in the panel's address. */
export const LABEL = 'label:';
export const labelSubject = (id: string) => `${LABEL}${id}`;
/** The label a subject names (its id for life, `la-<n>`), else null. */
export function labelOf(viewed: string | null): string | null {
  const id = viewed?.startsWith(LABEL) ? viewed.slice(LABEL.length) : null;
  return id && /^la-\d{1,15}$/.test(id) ? id : null;
}

/** A label's reference where it has no row yet (F9): its Discogs label id or its MBID. */
export type LabelRef = { source: 'discogs' | 'musicbrainz'; id: string };
/** A label as a record names it (F9): its name, the record's catalogue number there, its row's id where one is known (by its
 *  reference, or the one label of that name), else its reference. */
export type LabelChip = { name: string; catno?: string | null; id?: string; ref?: LabelRef };
/** Its chip: the name, and the catalogue number where there is one ("Warp · WARP123"). */
export const labelChip = (l: LabelChip) => (l.catno && l.catno.toLowerCase() !== 'none' ? `${l.name} · ${l.catno}` : l.name);

/** The labels a record's About shows (F9): its own (`labels`, the server's entries), else the album's label as its files or
 *  page name it, else Discogs'. */
export function aboutLabels(labels: readonly LabelChip[] | null | undefined, text: string | null | undefined, discogs: readonly LabelChip[] | null | undefined): LabelChip[] {
  if (labels?.length) return [...labels];
  if (text) return [{ name: text }];
  return [...(discogs ?? [])];
}
/** How a label in About opens (F9): its page by its row; found or made by its reference first; else not (plain text: a name two
 *  known labels share, or none knows). */
export const labelOpens = (l: LabelChip): 'page' | 'ref' | null => (l.id ? 'page' : l.ref ? 'ref' : null);
/** What finding or making a label's row by its reference sends (`POST /api/labels/open`). */
export type LabelOpenBody = { source: LabelRef['source']; id: string; name: string };
export const labelOpenBody = (l: LabelChip): LabelOpenBody => ({ source: l.ref!.source, id: l.ref!.id, name: l.name });

/** A row of a label's page, as the server answers it. */
export type LabelRow = {
  title: string;
  artist: string;
  year?: number | null;
  catno?: string | null;
  kind?: string | null;
  cover?: string | null;
  release_group?: string | null;
  discogs_master?: number | null;
  discogs_release?: number | null;
  /** The record that holds it (`al-<n>`); none until it is opened. */
  record?: string | null;
  had: boolean;
  /** An open request's mark for its record. */
  request?: RequestMark | null;
  /** How many editions of one album the row stands for (F12); absent where one. */
  editions?: number;
};
export type LabelSource = 'musicbrainz' | 'discogs';
/** A label's page (F10): what the house has (`had`, by the one had rule, at once) apart from the rest (`not_had`, its lists'),
 *  each by year; while its lists are read, `looking` and the source read now; a failed or switched-off read in F1's words. */
export type LabelPageOf = { id: string; name: string; had: LabelRow[]; not_had: LabelRow[]; looking: boolean; more: Record<LabelSource, boolean> } & ListRead;
/** Every record of the page, what the house has first: its picture's and its count's. */
export const labelRows = (p: Pick<LabelPageOf, 'had' | 'not_had'>): LabelRow[] => [...p.had, ...p.not_had];

/** A row's line: its artist, year, catalogue number, its type where it is not an album, and its editions where several (F12). */
export const rowLine = (r: LabelRow) =>
  [r.artist, r.year ? String(r.year) : null, r.catno || null, r.kind && r.kind !== 'Album' ? r.kind : null, editionsMark(r.editions)].filter(Boolean).join(' · ');

/** The sources whose lists read on: what "More" asks, one after another. */
export const moreToRead = (p: Pick<LabelPageOf, 'more'>): LabelSource[] => (['musicbrainz', 'discogs'] as const).filter((s) => p.more[s]);

/** Whether a row came from Discogs (its data shown with Discogs' attribution, its terms): one it knows, MusicBrainz not. */
export const fromDiscogs = (rows: readonly LabelRow[]) => rows.some((r) => !r.release_group && (!!r.discogs_master || !!r.discogs_release));

/** What a row opens by (lib/openRecord): its record, else its references; never its names alone. */
export const rowOpenable = (r: LabelRow): Openable => ({
  record: r.record,
  release_group: r.release_group,
  discogs_master: r.discogs_master,
  discogs_release: r.discogs_release,
  title: r.title,
  artist: r.artist,
  year: r.year,
});

/** The page's picture: its first four records' covers as a mosaic, else the first one's, else none. */
export function labelCovers(rows: readonly LabelRow[]): string[] {
  const covers = rows.flatMap((r) => (r.cover ? [r.cover] : []));
  return covers.length >= 4 ? covers.slice(0, 4) : covers.slice(0, 1);
}
