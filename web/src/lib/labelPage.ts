// A label's page (W11, C56): its records by year, newest first, had or not, with their catalogue numbers and requests, read from
// MusicBrainz's and Discogs' lists a hundred at a time (OQ10 (a): "More" reads on as the list's end comes into view). In the
// panel as `label:la-<n>`; a record's About names its labels as chips that open it. A row opens its record, made one at open
// from what its source knows it by and tied to the label (lib/openRecord). Pure.
import type { RequestMark } from './api';
import type { Openable } from './openRecord';

/** A label in the panel's address. */
export const LABEL = 'label:';
export const labelSubject = (id: string) => `${LABEL}${id}`;
/** The label a subject names (its id for life, `la-<n>`), else null. */
export function labelOf(viewed: string | null): string | null {
  const id = viewed?.startsWith(LABEL) ? viewed.slice(LABEL.length) : null;
  return id && /^la-\d{1,15}$/.test(id) ? id : null;
}

/** A label as a record names it: its id, name, and the record's catalogue number there. */
export type LabelChip = { id: string; name: string; catno?: string | null };
/** Its chip: the name, and the catalogue number where there is one ("Warp · WARP123"). */
export const labelChip = (l: LabelChip) => (l.catno && l.catno.toLowerCase() !== 'none' ? `${l.name} · ${l.catno}` : l.name);

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
};
export type LabelSource = 'musicbrainz' | 'discogs';
export type LabelPageOf = { id: string; name: string; rows: LabelRow[]; looking: boolean; more: Record<LabelSource, boolean> };

/** A row's line: its artist, year, catalogue number, and its type where it is not an album. */
export const rowLine = (r: LabelRow) => [r.artist, r.year ? String(r.year) : null, r.catno || null, r.kind && r.kind !== 'Album' ? r.kind : null].filter(Boolean).join(' · ');

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
