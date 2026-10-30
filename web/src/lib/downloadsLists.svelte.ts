// Downloads' lists as its page (the index) and its panel share them (the owner, 2026-10-24: these pages have their own
// panels, never the content panel): what is on this device, what is synced, what was downloaded once.
import { albumOfDownload } from './albums.svelte';
import { downloads, type DownloadedTrack } from './app.svelte';
import type { IconName } from './Icon.svelte';

/** The page's lists, each opened in the panel (`downloads:<id>`). */
export type DownloadsList = 'device' | 'synced' | 'once';
export const DOWNLOADS_LISTS: { id: DownloadsList; label: string; icon: IconName }[] = [
  { id: 'device', label: 'This device', icon: 'smartphone' },
  { id: 'synced', label: 'Synced', icon: 'refresh' },
  { id: 'once', label: 'Downloaded once', icon: 'download' },
];

export const WAITS = { wifi: 'Waiting for Wi-Fi', charging: 'Waiting to charge', network: 'Waiting for a connection' } as const;

/** A one-time copy's album (one compilation is one album, whatever its tracks' artists). */
export type OnceAlbum = { source: string; id: string; tracks: DownloadedTrack[] };

/** The one-time copies by album, the newest first. */
export function onceAlbums(): OnceAlbum[] {
  const by = new Map<string, OnceAlbum>();
  for (const t of downloads.tracks) {
    if (t.manual === false) continue;
    const id = albumOfDownload(t) ?? `${t.album ?? ''}\n${t.artist ?? ''}`;
    const key = `${t.source}/${id}`;
    const g = by.get(key) ?? { source: t.source, id, tracks: [] };
    g.tracks.push(t);
    by.set(key, g);
  }
  return [...by.values()].reverse();
}

/** An item where it lives: its own page, the item opened there (an album or an artist in the Library, a playlist in
 *  Playlists, a crate in Crates). */
export function goTo(kind: 'album' | 'artist' | 'playlist' | 'crate', subject: string) {
  const page = kind === 'crate' ? 'crates' : kind === 'playlist' ? '' : 'library';
  location.hash = `#/${page}?${new URLSearchParams({ view: subject })}`;
}
