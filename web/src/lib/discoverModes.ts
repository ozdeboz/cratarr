// Discover's modes, named once (the owner, 2026-10-29: Discover stays one page; what it shows is picked beside back, as
// the Library's records are, and the sidebar links to each): by the reason a record is suggested, each record in the
// one whose reason fits it best (the server's), and the account's Spotify lists (decision 0073, lib/spotify; the
// Library's before).
export const DISCOVER_MODES = [
  /** Anyone's records out in the last three months that fit, the newest first. */
  { value: 'fresh', label: 'Fresh' },
  /** Records of any age that fit what is played, the best fit first. */
  { value: 'alike', label: 'Alike' },
  /** Records missing from the library's artists' discographies, the most played artist's first. */
  { value: 'gaps', label: 'Gaps' },
  /** Artists the library does not have, each with a few of their records that fit best. */
  { value: 'artists', label: 'New artists' },
] as const;
export const SPOTIFY_VIEWS = [
  { value: 'spotify-albums', label: 'Albums' },
  { value: 'spotify-tracks', label: 'Tracks' },
  { value: 'spotify-playlists', label: 'Playlists' },
  { value: 'spotify-artists', label: 'Artists' },
] as const;
export type SpotifyView = (typeof SPOTIFY_VIEWS)[number]['value'];
export type ReasonMode = (typeof DISCOVER_MODES)[number]['value'];
export type DiscoverMode = ReasonMode | SpotifyView;
export const isSpotifyView = (v: unknown): v is SpotifyView => SPOTIFY_VIEWS.some((x) => x.value === v);
export const isDiscoverMode = (v: unknown): v is DiscoverMode => DISCOVER_MODES.some((x) => x.value === v) || isSpotifyView(v);
/** As much of a suggestion as the modes read (lib/api DiscoverAlbum). */
export type Moded = { why: string[]; new_artist: boolean; year: number | null; mode?: ReasonMode; score?: number; released?: string; artist_plays?: number };
/** Fresh's reach (the owner, 2026-10-29: the last three months). */
export const FRESH_DAYS = 90;
const DAY_MS = 86_400_000;
/** When it came out, where known (a month alone: its first day). */
const releasedAt = (s: Moded) => (s.released ? Date.parse(s.released.length === 7 ? `${s.released}-01` : s.released) : NaN);
/** The mode a suggestion is in: the server's (the reason that fits it best); from a server before it, worked out from
 *  why it was suggested, in the server's tie order (fresh, gaps, artists, alike). */
export function modeOf(s: Moded, now = Date.now()): ReasonMode {
  if (s.mode) return s.mode;
  const at = releasedAt(s);
  if (s.why.some((w) => w.startsWith('New from ')) || now - at < FRESH_DAYS * DAY_MS) return 'fresh';
  if (s.why.some((w) => w.startsWith('Missing from '))) return 'gaps';
  if (s.new_artist) return 'artists';
  return 'alike';
}
/** A mode's suggestions in its order: Fresh the newest first, Gaps the most played artist's first, Alike and New
 *  artists the best fit first; equals keep the server's order. */
export function inModeOrder<S extends Moded>(mode: ReasonMode, list: S[]): S[] {
  const by =
    mode === 'fresh'
      ? (s: S) => {
          const at = releasedAt(s);
          return Number.isNaN(at) ? (s.year ? Date.UTC(s.year, 0, 1) : 0) : at;
        }
      : mode === 'gaps'
        ? (s: S) => s.artist_plays ?? 0
        : (s: S) => s.score ?? 0;
  return list.map((s, i) => ({ s, i, k: by(s) })).sort((a, b) => b.k - a.k || a.i - b.i).map((x) => x.s);
}
/** A mode as the picker and the sidebar name it. */
export function modeLabel(m: DiscoverMode): string {
  const s = SPOTIFY_VIEWS.find((x) => x.value === m);
  return s ? `Spotify ${s.label.toLowerCase()}` : (DISCOVER_MODES.find((x) => x.value === m)?.label ?? 'Fresh');
}
