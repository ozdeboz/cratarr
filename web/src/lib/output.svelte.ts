// Where music plays (decisions 0019, 0022): on a media server's player (remote: today's Finamp on the tablet) or in
// this browser (local, lib/localPlayer.svelte.ts). The player shows either the same way: the local queue is presented
// as a now-playing view like the server's. Switching moves the music: the queue and the position go to the other side,
// which plays from there, and the side that played stops.
import { handover, incomingTrackIds, playlistTrackIds, trackDetails, transport, type NowPlaying, type PlayMode } from './api';
import { live, refresh } from './live.svelte';
import { local, playQueue, enqueue, seek, stop, type LocalTrack } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';

export const isLocal = () => prefs.output === 'local';

/** The local queue's playing track as a now-playing view (the shape the player shows). */
export function localNowPlaying(): NowPlaying | null {
  const t = local.queue[local.index];
  if (!t) return null;
  return {
    source: t.source,
    id: t.id,
    name: t.name || '…',
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
    addable: t.addable ?? true,
    progress: { position_secs: local.position, duration_secs: local.duration, paused: local.paused, estimated: false },
    player: 'This device',
    favorite: null,
    upcoming: local.queue.slice(local.index + 1).map((u) => ({
      id: u.id,
      name: u.name || '…',
      artist: u.artist,
      album: u.album,
      album_id: u.album_id,
      addable: u.addable ?? true,
      duration_secs: null,
    })),
  };
}

// Track details (title, artist, album) arrive lazily: a playlist starts with ids only; the playing track and the next
// few are filled in as the queue moves.
const asked = new Set<string>();
async function fill(from: number) {
  for (let i = Math.max(0, from); i < Math.min(local.queue.length, from + 4); i++) {
    const t = local.queue[i];
    const key = `${t.source}/${t.id}`;
    if (t.name || asked.has(key)) continue;
    asked.add(key);
    try {
      const d = await trackDetails(t.source, t.id);
      const j = local.queue.findIndex((q) => q.source === t.source && q.id === t.id && !q.name);
      if (j >= 0) local.queue[j] = { ...local.queue[j], name: d.name, artist: d.artist, album: d.album, album_id: local.queue[j].album_id ?? d.album_id };
    } catch {
      asked.delete(key);
    }
  }
}
$effect.root(() => {
  $effect(() => {
    void local.index;
    void local.queue.length;
    fill(local.index);
  });
});

/** Plays a playlist where the output is: the server's player, or here (its tracks fetched, then the details). */
export async function playPlaylistHere(id: string, mode: PlayMode): Promise<boolean> {
  if (!isLocal()) return false;
  const { source, ids } = await playlistTrackIds(id, prefs.source || undefined);
  if (!ids.length) throw new Error('None of its tracks can be found.');
  const order = mode === 'shuffle' ? [...ids].sort(() => Math.random() - 0.5) : ids;
  const tracks: LocalTrack[] = order.map((tid) => ({ source, id: tid, name: '', artist: null, album: null, album_id: null }));
  if (mode === 'next' || mode === 'last') enqueue(tracks, mode);
  else playQueue(tracks, 0);
  return true;
}

/** Plays an Incoming album here when the output is this browser (its tracks are not in Music: never addable). */
export async function playIncomingHere(source: string, album: string): Promise<boolean> {
  if (!isLocal()) return false;
  const r = await incomingTrackIds(source, album);
  if (!r.ids.length) throw new Error('That album has no tracks.');
  playQueue(r.ids.map((id) => ({ source: r.source, id, name: '', artist: null, album: null, album_id: album, addable: false })), 0);
  return true;
}

/** Moves the music here: what the server's player plays (and what comes next) from where it is; the player stops. */
export async function toLocal() {
  const np = live.view?.now_playing ?? null;
  prefs.output = 'local';
  if (!np) return;
  const tracks: LocalTrack[] = [np, ...np.upcoming].map((t) => ({
    source: np.source,
    id: t.id,
    name: t.name,
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
  }));
  const at = np.progress?.position_secs ?? 0;
  try {
    await transport('stop', np.source);
  } catch {
    /* it may have stopped already */
  }
  playQueue(tracks, 0);
  if (at > 1) setTimeout(() => seek(at), 300);
}

/** Moves the music to the server's player: the local queue from the playing track and position; here stops. */
export async function toRemote() {
  if (local.index < 0) {
    prefs.output = 'remote';
    return;
  }
  const source = local.queue[local.index]?.source;
  const was = stop();
  prefs.output = 'remote';
  try {
    await handover(was.queue.map((t) => t.id), was.index, was.position, source);
    setTimeout(refresh, 700);
  } catch (e) {
    toast.fail(e, "The music didn't move to the player");
  }
}
