// Export's choices (decision 0058, lib/ExportStep): the format, the layout and what goes along, as the step offers
// them; the request they make; and the line saying what the export will hold. No runes, so the tests take it as it is.
import type { ExportFormat, ExportLayout, ExportPreview, ExportRequest, ExportWhat } from './api';

export type ExportOptions = {
  /** Playlists: only the playlist files, not their tracks (the owner, 2026-11-02: "Playlist only", off as it comes). */
  playlistOnly: boolean;
  /** Several playlists: combined into one. */
  combine: boolean;
  /** The combined playlist's name; it follows the selection until typed in (`named`). */
  name: string;
  named: boolean;
  /** A list of tracks: with a playlist file of them. */
  m3u: boolean;
  format: ExportFormat;
  /** Opus's, kb/s. */
  bitrate: number;
  layout: ExportLayout;
  /** Download…'s first choice in the app (and an installed page): to this device, or as files (0.10). */
  where: 'device' | 'files';
};

export const FORMATS: { value: ExportFormat; label: string }[] = [
  { value: 'original', label: 'Original files' },
  { value: 'flac-cd', label: 'FLAC at CD quality' },
  { value: 'opus', label: 'Opus' },
  // 0.10 (the owner, 2026-10-30): MP3 too, beside the ones before.
  { value: 'mp3-320', label: 'MP3 at 320 kb/s' },
  { value: 'mp3-v0', label: 'MP3 V0' },
];
export const BITRATES = [96, 128, 160, 256];
export const LAYOUTS: { value: ExportLayout; label: string }[] = [
  { value: 'flat', label: 'One folder' },
  { value: 'folders', label: 'Artist and album folders' },
];

export const blankExport = (): ExportOptions => ({
  playlistOnly: false,
  combine: false,
  name: '',
  named: false,
  m3u: false,
  format: 'original',
  bitrate: 160,
  layout: 'folders',
  where: 'device',
});

/** The choices as kept in this browser, each that is not one of today's left at its default; an older version's "All
 *  tracks in one folder" is the one-folder layout. The old "Include tracks" is not read: Playlist only starts off. */
export function keptExport(k: unknown): ExportOptions {
  const out = blankExport();
  if (!k || typeof k !== 'object') return out;
  const o = k as Record<string, unknown>;
  for (const key of ['playlistOnly', 'combine', 'named', 'm3u'] as const) if (typeof o[key] === 'boolean') out[key] = o[key];
  if (typeof o.name === 'string') out.name = o.name;
  if (FORMATS.some((f) => f.value === o.format)) out.format = o.format as ExportFormat;
  if (typeof o.bitrate === 'number' && BITRATES.includes(o.bitrate)) out.bitrate = o.bitrate;
  if (LAYOUTS.some((l) => l.value === o.layout)) out.layout = o.layout as ExportLayout;
  else if (o.flat === true) out.layout = 'flat';
  if (o.where === 'device' || o.where === 'files') out.where = o.where;
  return out;
}

/** Whether the export carries tracks: a list of tracks always does, playlists unless only their files. */
export const withTracks = (what: ExportWhat, o: ExportOptions) => what.kind === 'tracks' || !o.playlistOnly;

/** A list of tracks may take a playlist file of them; one track does not (the owner, 2026-11-02). */
export const playlistFileOffered = (what: ExportWhat) => what.kind === 'tracks' && what.tracks.length > 1;

/** Several playlists, which may be combined into one. */
export const several = (what: ExportWhat) => what.kind === 'playlists' && what.names.length > 1;

export function exportRequest(what: ExportWhat, o: ExportOptions): ExportRequest {
  return {
    what,
    format: o.format,
    bitrate: o.bitrate,
    layout: o.layout,
    tracks: withTracks(what, o),
    combine: several(what) && o.combine ? o.name.trim() : null,
    m3u: playlistFileOffered(what) && o.m3u,
  };
}

/** Something to export, and a combined playlist with a name. */
export const exportReady = (r: ExportRequest) =>
  r.combine !== '' && (r.what.kind === 'playlists' ? r.what.names.length > 0 : r.what.tracks.length > 0);

export const size = (b: number) =>
  b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : b >= 1e6 ? `${Math.round(b / 1e6)} MB` : `${Math.max(1, Math.round(b / 1e3))} kB`;

/** The Download button's words, as offline listening's ("Download 312 tracks · about 9.4 GB"; the owner, 2026-11-02:
 *  the size on the button, not in a line above), only the playlist files counted where the tracks stay out. */
export function exportLabel(p: ExportPreview | null, r: ExportRequest): string {
  if (!r.tracks) {
    const files = r.combine !== null || r.what.kind !== 'playlists' ? 1 : r.what.names.length;
    return files === 1 ? 'Download the playlist file' : `Download ${files.toLocaleString()} playlist files`;
  }
  if (!p) return 'Download';
  const n = `Download ${p.tracks.toLocaleString()} ${p.tracks === 1 ? 'track' : 'tracks'}`;
  return p.bytes !== null ? `${n} · about ${size(p.bytes)}` : n;
}

/** Under it, what stays out: "3 missing are left out". */
export const exportLeftOut = (p: ExportPreview | null) =>
  p && p.missing > 0 ? `${p.missing.toLocaleString()} missing ${p.missing === 1 ? 'is' : 'are'} left out` : null;

/** The combined playlist's name from the playlists': up to three joined, else the first two and how many more. */
export const suggestedName = (names: string[]) =>
  names.length <= 3 ? names.join(' + ') : `${names.slice(0, 2).join(' + ')} and ${names.length - 2} more`;
