// Away from Cratarr (lib/reach: out of reach; the owner, 2026-10-28: "the best PWA offline support possible"): what is
// on this device plays, and what is not shows so. On the device: in the app its downloads, kept and once alike (the
// app's `downloads` report); in the installed page the tracks it keeps (lib/offline). What plays is picked here, as the
// server would (lib/play's one way to play): from what the pages last read of the collection (public/sw.js keeps it),
// those tracks on the device alone, in their order or mixed.
import { ApiError, albumView, artistView, crateRows, playlistRows, type Picked, type Picking, type PlayHow, type PlaylistRow } from './api';
import { downloads, inApp, onApp } from './app.svelte';
import { DEVICE } from './deviceMusic.svelte';
import { offline } from './offline.svelte';
import { reach, unreached } from './reach.svelte';
import { shuffled } from './shuffle';

export const NOT_HERE = 'Not on this device';

/** The device's network, as the app reports it (0.9.15): `online` some network carries traffic (a home Wi-Fi without
 *  the internet too: Cratarr may be on it), `validated` Android found the internet on it. No network: out of reach for
 *  sure, at once, before any request finds out. */
export const network = $state({ online: null as boolean | null, validated: null as boolean | null, kind: null as string | null });
if (inApp)
  onApp('network', (m) => {
    network.online = typeof m.online === 'boolean' ? m.online : null;
    network.validated = typeof m.validated === 'boolean' ? m.validated : null;
    network.kind = typeof m.kind === 'string' ? m.kind : null;
    if (network.online === false) unreached();
  });

/** Each track on the device (`source/id`), and how many of each album's (`source/album_id`). */
const onDevice = $derived.by(() => {
  const tracks = new Set<string>();
  const albums = new Map<string, number>();
  const one = (source: string, id: string, album: string | null) => {
    tracks.add(`${source}/${id}`);
    if (album) albums.set(`${source}/${album}`, (albums.get(`${source}/${album}`) ?? 0) + 1);
  };
  if (inApp) for (const t of downloads.tracks) t.download.state === 'done' && one(t.source, t.id, t.album_id);
  else for (const t of Object.values(offline.kept)) one(t.source, t.id, t.album_id);
  return { tracks, albums };
});

/** Out of reach: only what is on the device plays. */
export const away = () => reach.away;
/** A track is on this device. */
export const here = (source: string, id: string | null | undefined) => !!id && (source === DEVICE || onDevice.tracks.has(`${source}/${id}`));
/** How many of an album's tracks are on this device. */
export const albumHere = (source: string, album: string) => onDevice.albums.get(`${source}/${album}`) ?? 0;
/** Out of reach, and none of the album on the device: it cannot play (its card dimmed, its Play gone). */
export const albumAway = (source: string, album: string) => reach.away && source !== DEVICE && albumHere(source, album) === 0;
/** Out of reach, and none of these tracks on the device. */
export const rowsAway = (source: string, ids: (string | null | undefined)[]) => reach.away && !ids.some((id) => here(source, id));

/** The collection's rows as last read (its kept copy, the server out of reach), with their source. */
async function rowsOf(what: Picking, source: string | undefined): Promise<{ source: string; rows: PlaylistRow[] } | null> {
  switch (what.kind) {
    case 'album': {
      const v = await albumView(what.source, what.id);
      return { source: v.source, rows: v.rows.map((r) => ({ ...r, album_id: r.album_id ?? what.id })) };
    }
    case 'artist': {
      const v = await artistView(what.source, what.name);
      return { source: v.source, rows: v.rows };
    }
    case 'playlist':
      return playlistRows(what.id, source);
    case 'crate': {
      const c = 'favourites' in what ? what : null;
      const r = await crateRows(what.id, 0, 500, source, c?.favourites, c?.favourite_albums, c ? { without: c.without, filter: c.filter, order: c.order, down: c.down } : {});
      return { source: r.source, rows: r.rows };
    }
    case 'tracks':
      return null;
  }
}

/** What plays of `what` with Cratarr out of reach: its tracks on this device, in its order from `from` (or mixed). */
export async function pickHere(what: Picking, how: PlayHow, from: string | undefined, source: string | undefined): Promise<Picked> {
  let got: { source: string; rows: PlaylistRow[] } | null = null;
  try {
    got = await rowsOf(what, source);
  } catch {
    /* never read here: what the device holds of it, below */
  }
  type T = Picked['tracks'][number];
  let tracks: T[];
  if (got) {
    const src = got.source;
    tracks = got.rows.flatMap((r, n) =>
      here(src, r.id) && !r.missing
        ? [{ source: src, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: r.album_id ?? null, duration_secs: r.duration_secs, n }]
        : [],
    );
  } else {
    // The device's own list: a collection never read here (an album's tracks by its id), or tracks named by id.
    const all = inApp
      ? downloads.tracks.filter((t) => t.download.state === 'done').map((t) => ({ source: t.source, id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id }))
      : Object.values(offline.kept).map((t) => ({ source: t.source, id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id, duration_secs: t.duration_secs }));
    const pick =
      what.kind === 'tracks'
        ? what.ids.flatMap((id) => all.filter((t) => t.source === what.source && t.id === id))
        : what.kind === 'album'
          ? all.filter((t) => t.source === what.source && t.album_id === what.id)
          : what.kind === 'artist'
            ? all.filter((t) => t.source === what.source && t.artist === what.name)
            : [];
    tracks = pick.map((t, n) => ({ ...t, n }));
  }
  if (!tracks.length) throw new ApiError(NOT_HERE);
  if (how === 'shuffle') return { source: tracks[0].source, tracks: shuffled(tracks), index: 0, total: tracks.length };
  const at = from ? Math.max(0, tracks.findIndex((t) => t.id === from)) : 0;
  return { source: tracks[0].source, tracks, index: at, total: tracks.length };
}
