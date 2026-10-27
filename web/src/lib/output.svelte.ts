// Where music plays (decisions 0019, 0022, 0031): on a media server's player (remote: Finamp on the tablet), in this
// browser or app (local, lib/localPlayer.svelte.ts), or on one of Cratarr's own players (Play On, lib/playOn.svelte.ts),
// which the page drives as it drives the local player. The player shows each the same way: the queue the page holds is
// presented as a now-playing view like the server's. Switching moves the music: the queue and the position go to the
// other side, which plays from there, and the side that played stops.
import {
  albumView,
  handover,
  playPlaylist,
  playlistRows,
  playlistTrackIds,
  queueTracks,
  setMember,
  setTrackMember,
  trackDetails,
  reportPlays,
  trackState,
  transport,
  type HeardEvent,
  type NowPlaying,
  type PlayMode,
  type Playlist,
  type TrackState,
} from './api';
import { untrack } from 'svelte';
import { live, refresh, untilNewTrack } from './live.svelte';
import { canPlayHere, keptHere, local, playQueue, enqueue, stop, togglePlay, type LocalTrack } from './localPlayer.svelte';
import { inApp } from './app.svelte';
import { rewindNow } from './ramping.svelte';
import { drivenName, follow, outputName, playOn, pointAt } from './playOn.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';

// Whether the page holds the queue: it plays here, or on one of Cratarr's players it drives. A browser that cannot play
// here (no stream for audio) plays on a player, whatever was chosen elsewhere.
export const isLocal = () => (prefs.output === 'local' && canPlayHere) || drivenName() !== null;

// "The server's player" while one of Cratarr's own players plays: that player, driven. Cratarr's own source has no
// server-side player; such a page only showed what that player played, without seek or queue edits, while its output
// read like the player's (2026-10-16).
if (typeof window !== 'undefined') {
  $effect.root(() => {
    $effect(() => {
      if (prefs.output !== 'remote') return;
      const name = live.view?.now_playing?.player;
      if (name && playOn.players.some((p) => p.name === name && p.online)) {
        untrack(() => {
          pointAt(`player:${name}`);
          follow(name);
        });
      }
    });
  });
}

/** Whether the music plays in this page (its stream, or the app it runs in). */
export const playsHere = () => prefs.output === 'local' && canPlayHere;

// Leaving the page stops what plays in this browser, so a reload or closing the tab asks first (the browser's own
// "Leave site?"), but only while this browser is playing.
addEventListener('beforeunload', (e) => {
  if (!playsHere() || local.index < 0 || local.paused) return;
  e.preventDefault();
  e.returnValue = '';
});

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
    addable: localState.view?.addable ?? t.addable ?? true,
    progress: { position_secs: local.position, duration_secs: local.duration, paused: local.paused, estimated: false },
    player: outputName(),
    favorite: localState.view?.favorite ?? null,
    // As the heart, the chips stay until the new track's arrive (no row coming and going between tracks).
    info: localState.view?.info ?? null,
    upcoming: local.queue.slice(local.index + 1).map((u) => ({
      id: u.id,
      name: u.name || '…',
      artist: u.artist,
      album: u.album,
      album_id: u.album_id,
      addable: u.addable ?? true,
      duration_secs: u.duration_secs ?? null,
      lane: u.lane,
    })),
  };
}

// The local track's playlists and favourite: the server's state covers only its own player's track, so they are
// asked for this one (on each new track, and when the playlists change).
// `loading`: the playing track's are asked for. Meanwhile the track before's stay shown (the heart, the playlists),
// disabled: the same answer, as between tracks of one album, then shows no change at all.
export const localState = $state<{ key: string | null; view: TrackState | null; loading: boolean }>({
  key: null,
  view: null,
  loading: false,
});
const keyOf = (t: LocalTrack) => `${t.source}/${t.id}`;
async function loadState(t: LocalTrack) {
  try {
    const view = await trackState(t.source, t.id);
    if (localState.key !== keyOf(t)) return;
    localState.view = view;
  } catch {
    /* shown as in no playlist, without a heart */
    if (localState.key === keyOf(t)) localState.view = null;
  } finally {
    if (localState.key === keyOf(t)) localState.loading = false;
  }
}
$effect.root(() => {
  $effect(() => {
    const t = local.queue[local.index];
    const key = t ? keyOf(t) : null;
    if (key === localState.key) return;
    localState.key = key;
    if (!t) localState.view = null;
    localState.loading = !!t;
    if (t) loadState(t);
  });
  // Playlists made, renamed or changed elsewhere: the server's state says so (by their counts and names).
  let seen = '';
  $effect(() => {
    const sig = (live.view?.playlists ?? []).map((p) => `${p.id}:${p.count}`).join('|');
    const t = local.queue[local.index];
    if (sig === seen) return;
    const first = !seen;
    seen = sig;
    if (!first && t && isLocal()) loadState(t);
  });
});

/** The playlists as shown with what plays: membership of the server's player's track, or of the local one. */
export function shownPlaylists(): Playlist[] {
  if (!isLocal()) return live.view?.playlists ?? [];
  return localState.view?.playlists ?? (live.view?.playlists ?? []).map((p) => ({ ...p, member: false }));
}
/** Whether `shownPlaylists` has been read: until then its [] is not known to be empty, and a count says nothing
 *  rather than "0 tracks" (the owner, 2026-10-24). */
export const playlistsKnown = () => !!live.view || (isLocal() && !!localState.view);

/** Puts what plays in the playlist or takes it out, where the output is. */
export async function setPlayingMember(playlistId: string, member: boolean) {
  if (!isLocal()) {
    live.view = await setMember(playlistId, member, prefs.source || undefined);
    return;
  }
  const t = local.queue[local.index];
  if (!t) throw new Error('Nothing is playing.');
  const view = await setTrackMember(t.source, t.id, playlistId, member);
  if (localState.key === keyOf(t)) localState.view = view;
}

/** The local favourite changed: the answer, if it is still about the playing track. */
export function tookLocal(source: string, id: string, view: TrackState) {
  if (localState.key === `${source}/${id}`) localState.view = view;
}

// A track the engine could not play is skipped (as a player does); it says which and why, once. The line goes once a
// later track plays (the engine clears the error), unless another toast replaced it meanwhile.
let errorToast: number | null = null;
$effect.root(() => {
  $effect(() => {
    const e = local.error;
    untrack(() => {
      if (e) {
        toast.show({ text: e, tone: 'error' });
        errorToast = toast.current?.id ?? null;
      } else if (errorToast !== null) {
        if (toast.current?.id === errorToast) toast.dismiss();
        errorToast = null;
      }
    });
  });
});

// Every track heard is told (decision 0064). A play counts (decision 0022) once a track has played 90% through
// (Jellyfin's own "played" mark), once per time it is reached in the queue: told then, as finished. A track left
// before that is told when it is left: skipped for another track, stopped when the queue ends; one heard for less than
// a second is not. The server keeps it for the account it plays as.
const PLAYED_AT = 0.9;
/** The queue place being heard (index and track), from when, how far it got, and whether it was told as counted. */
type Hearing = { at: string; track: LocalTrack; started: number; heard: number; length: number | null; counted: boolean };
let hearing: Hearing | null = null;
/** Reports not yet taken (the server away): sent again with the next one; the latest 500 kept. */
let unsent: HeardEvent[] = [];
/** A report's own id; `crypto.randomUUID` needs a secure page (a LAN address is plain http). */
const reportId = () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`);
function tell(h: Hearing, ended: HeardEvent['ended'], counts: boolean) {
  unsent.push({
    id: reportId(),
    source: h.track.source,
    track: h.track.id,
    at: h.started,
    ended,
    counts,
    heard_secs: Math.round(h.heard),
    length_secs: h.length === null ? null : Math.round(h.length),
    player: prefs.browserName || null,
    context: keptHere.id ? `queue:${keptHere.id}` : null,
  });
  const batch = unsent.slice(-500);
  unsent = [];
  reportPlays(batch).then(
    (r) => unsent.push(...batch.filter((e) => !r.done.includes(e.id))),
    () => unsent.push(...batch),
  );
}
$effect.root(() => {
  $effect(() => {
    const t = local.queue[local.index];
    const at = `${local.index}/${t ? keyOf(t) : ''}`;
    // Told where it plays: here; a player of Cratarr's tells its own, and so does the app this page may run in
    // (decision 0033: offline and behind a locked screen too).
    if (!playsHere() || inApp) {
      hearing = null;
      return;
    }
    // Winding is no listening (decision 0074), forward or back, nor are the tracks a wind passes through the queue: the
    // hearing waits until it plays on, where it landed.
    if (rewindNow.on) return;
    if (hearing && hearing.at !== at) {
      if (!hearing.counted && hearing.heard >= 1) tell(hearing, t ? 'skipped' : 'stopped', false);
      hearing = null;
    }
    if (!t) return;
    hearing ??= { at, track: t, started: Date.now(), heard: 0, length: null, counted: false };
    hearing.heard = Math.max(hearing.heard, local.position);
    hearing.length = local.duration ?? hearing.length;
    const d = local.duration;
    if (hearing.counted || !d || local.position < d * PLAYED_AT) return;
    hearing.counted = true;
    tell(hearing, 'finished', true);
  });
});

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
  // With their names: a player elsewhere (the app, a Cratarr player) shows them at once, on its lock screen too.
  const { source, rows } = await playlistRows(id, prefs.source || undefined);
  const named: LocalTrack[] = rows
    .filter((r) => r.id && !r.missing)
    .map((r) => ({ source, id: r.id!, name: r.name ?? '', artist: r.artist, album: r.album, album_id: null }));
  if (!named.length) throw new Error('None of its tracks can be found.');
  if (mode === 'next' || mode === 'last') enqueue(named, mode);
  else playQueue(named, 0, id, 0, mode === 'shuffle');
  return true;
}

/** Plays a playlist where the output is (its menu's Play, Shuffle, Play next and Add to queue; its view's buttons):
 *  here, or on the server's player, showing the new track once it is there for Play and Shuffle. */
export async function startPlaylist(id: string, mode: PlayMode) {
  if (await playPlaylistHere(id, mode)) return;
  await playPlaylist(id, mode, prefs.source || undefined);
  if (mode === 'play' || mode === 'shuffle') await untilNewTrack();
  else refresh();
}

/** Several playlists (the selection's ⋮, decision 0025): their tracks one playlist after another, in the order given,
 *  each once, here or on the server's player. */
export async function startPlaylists(ids: string[], mode: PlayMode) {
  // What plays is the server's choice (500 at most, a shuffle drawn from all of them; lib/play): one playlist by its
  // id, several as their tracks, each once, in their order (the owner, 2026-10-23: a playlist's and a multi-select's
  // Play and Shuffle too).
  const { playCollection } = await import('./play.svelte');
  if (ids.length === 1) return playCollection({ kind: 'playlist', id: ids[0] }, mode, ids[0]);
  let source = '';
  const seen = new Set<string>();
  const all: string[] = [];
  for (const id of ids) {
    const r = await playlistTrackIds(id, prefs.source || undefined);
    source = r.source;
    for (const t of r.ids) if (!seen.has(t)) seen.add(t) && all.push(t);
  }
  if (!all.length) throw new Error('None of their tracks can be found.');
  await playCollection({ kind: 'tracks', source, ids: all }, mode, ids.join(', '));
}

/** Plays a playlist from one of its tracks (a tap on a row of its view, decision 0023): these tracks from `index`, here
 *  or on the server's player. */
export async function playPlaylistFrom(name: string, source: string, tracks: LocalTrack[], index: number, position = 0) {
  if (isLocal()) return playQueue(tracks, index, name, position);
  await handover(
    tracks.map((t) => t.id),
    index,
    position,
    source,
  );
  setTimeout(refresh, 700);
}

/** Plays an album here when the output is this browser. Whether its tracks can go in a playlist (in Music, not
 *  Incoming) the server says with each track's state. */
export async function playAlbumHere(source: string, album: string, title?: string): Promise<boolean> {
  if (!isLocal()) return false;
  const r = await albumView(source, album);
  if (!r.rows.length) throw new Error('That album has no tracks.');
  playQueue(
    r.rows.map((x) => ({ source: r.source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: album, duration_secs: x.duration_secs })),
    0,
    title ?? r.title,
  );
  return true;
}

/** What plays where the output is now, to move it: the queue, the place in it and the position (s). */
function current(): { tracks: LocalTrack[]; index: number; position: number } | null {
  if (isLocal()) return local.index < 0 ? null : { tracks: [...local.queue], index: local.index, position: local.position };
  const np = live.view?.now_playing ?? null;
  if (!np) return null;
  const tracks: LocalTrack[] = [np, ...np.upcoming].map((t) => ({
    source: np.source,
    id: t.id,
    name: t.name,
    artist: t.artist,
    album: t.album,
    album_id: t.album_id,
  }));
  return { tracks, index: 0, position: np.progress?.position_secs ?? 0 };
}

/** Plays on `output` from now on, and moves what plays there: the queue from the playing track, at its position; the
 *  side that played stops. */
export async function moveTo(output: typeof prefs.output) {
  if (output === prefs.output) return;
  const was = current();
  if (isLocal()) {
    stop();
  } else if (was) {
    try {
      await transport('stop', was.tracks[0].source);
    } catch {
      /* it may have stopped already */
    }
  }
  pointAt(output);
  follow(output.startsWith('player:') ? output.slice('player:'.length) : null);
  if (!was) return;
  if (output === 'remote') {
    await handover(was.tracks.map((t) => t.id), was.index, was.position, was.tracks[0].source);
    setTimeout(refresh, 700);
    return;
  }
  playQueue(was.tracks, was.index, null, was.position > 1 ? was.position : 0, false, true);
}

/** Whether music plays (or waits, paused) where the music plays now: what a move would take along. */
export const hasMusic = () => current() !== null;

/** Plays on `output` from now on without moving anything there: what plays there plays on, and what played here pauses
 *  (the owner chose not to take that player over). */
export async function joinTo(output: typeof prefs.output) {
  if (output === prefs.output) return;
  if (isLocal()) {
    if (local.index >= 0 && !local.paused) togglePlay(true);
  } else {
    const np = live.view?.now_playing ?? null;
    if (np && !np.progress?.paused) {
      try {
        await transport('play-pause', np.source);
      } catch {
        /* it may have stopped already */
      }
    }
  }
  pointAt(output);
  follow(output.startsWith('player:') ? output.slice('player:'.length) : null);
}

/** Moves the music here. */
export const toLocal = () => moveTo('local');

/** Moves the music to the server's player. */
export const toRemote = () => moveTo('remote');
