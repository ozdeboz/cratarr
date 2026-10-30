// Where music plays (decisions 0022, 0031): in this browser or app (local, lib/localPlayer.svelte.ts), or on one of
// Cratarr's own players (Play On, lib/playOn.svelte.ts), which the page drives as it drives the local player. The media
// server's player (remote, decision 0019) went from Cratarr's mode in 0.12; a Jellyfin mode lists that server's own
// players by name (0.13). A browser that cannot play here with no player chosen has nowhere to play (lib/player/port). The player shows each the same way: the queue the page holds is
// presented as a now-playing view like the server's. Switching moves the music: the queue and the position go to the
// other side, which plays from there, and the side that played stops.
import { away, pickHere } from './away.svelte';
import { DEVICE, deviceAlbumView } from './deviceMusic.svelte';
import { albumView, playlistTrackIds, queueTracks, trackDetails, reportPlays, type HeardEvent, type PlayMode, type Playlist } from './api';
import { untrack } from 'svelte';
import { canPlayHere, keptHere, local, playQueue, type LocalTrack } from './localPlayer.svelte';
import { inApp } from './app.svelte';
import { rewindNow } from './ramping.svelte';
import { drivenName, follow, playOn, pointAt } from './playOn.svelte';
import { port } from './player/port.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';
import { keep, readKept } from './keptStore';
import { reach, whenReached } from './reach.svelte';

// Which player the page drives, and what this device's queue shows of what plays: the port's (lib/player/port).
export { isLocal, localNowPlaying, localState, tookLocal } from './player/port.svelte';

/** Whether the music plays in this page (its stream, or the app it runs in). */
export const playsHere = () => prefs.output === 'local' && canPlayHere;

// Leaving the page stops what plays in this browser, so a reload or closing the tab asks first (the browser's own
// "Leave site?"), but only while this browser is playing.
addEventListener('beforeunload', (e) => {
  if (!playsHere() || local.index < 0 || local.paused) return;
  e.preventDefault();
  e.returnValue = '';
});


/** The playlists as shown with what plays: membership of the server's player's track, or of the local one. */
export const shownPlaylists = (): Playlist[] => port().playlists();
/** Whether `shownPlaylists` has been read: until then its [] is not known to be empty, and a count says nothing
 *  rather than "0 tracks" (the owner, 2026-10-24). */
export const playlistsKnown = () => port().playlistsKnown();

/** Puts what plays in the playlist or takes it out, where the output is. */
export const setPlayingMember = (playlistId: string, member: boolean) => port().setMember(playlistId, member);

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
const keyOf = (t: LocalTrack) => `${t.source}/${t.id}`;
/** The queue place being heard (index and track), from when, how far it got, and whether it was told as counted. */
type Hearing = { at: string; track: LocalTrack; started: number; heard: number; length: number | null; counted: boolean };
let hearing: Hearing | null = null;
/** Reports not yet taken (the server away): kept on this device (lib/keptStore; 0.12, decision 0085 step 3: no play
 *  lost when the page is closed meanwhile) and sent again with the next one or when Cratarr answers again; the latest
 *  500 kept. Each has its id: Cratarr counts a report sent twice once. `telling`: those on their way. */
let unsent: HeardEvent[] = [];
let telling: HeardEvent[] = [];
const PLAYS = 'plays';
const keepPlays = () => void keep(PLAYS, [...telling, ...unsent].slice(-500));
function tellNow() {
  if (telling.length || !unsent.length) return;
  telling = unsent.slice(-500);
  unsent = [];
  let taken = false;
  reportPlays(telling)
    .then(
      (r) => {
        taken = true;
        unsent = [...telling.filter((e) => !r.done.includes(e.id)), ...unsent];
      },
      () => (unsent = [...telling, ...unsent]),
    )
    .finally(() => {
      telling = [];
      keepPlays();
      if (taken && unsent.length) tellNow();
    });
}
if (typeof window !== 'undefined') {
  readKept<HeardEvent[]>(PLAYS).then((v) => {
    if (!Array.isArray(v) || !v.length) return;
    const have = new Set([...telling, ...unsent].map((e) => e.id));
    unsent = [...v.filter((e) => !have.has(e.id)), ...unsent];
    if (!reach.away) tellNow();
  });
  whenReached.add(tellNow);
}
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
  keepPlays();
  tellNow();
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

/** Plays a playlist where the page plays (its tracks fetched, then the details); false with no player to play on. */
export async function playPlaylistHere(id: string, mode: PlayMode): Promise<boolean> {
  const p = port();
  if (!p.holdsQueue) return false;
  await p.playPlaylist(id, mode);
  return true;
}

/** Plays a playlist where the output is (its menu's Play, Shuffle, Play next and Add to queue; its view's buttons):
 *  here, or on the server's player, showing the new track once it is there for Play and Shuffle. */
export const startPlaylist = async (id: string, mode: PlayMode) => port().playPlaylist(id, mode);

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
export const playPlaylistFrom = async (name: string, source: string, tracks: LocalTrack[], index: number, position = 0) => port().playFrom(name, source, tracks, index, position);

/** Plays an album here when the output is this browser. Whether its tracks can go in a playlist (in Music, not
 *  Incoming) the server says with each track's state. */
export async function playAlbumHere(source: string, album: string, title?: string): Promise<boolean> {
  if (!port().holdsQueue) return false;
  // Out of reach (lib/away): its tracks on this device alone.
  if (away()) {
    const got = await pickHere({ kind: 'album', source, id: album }, 'play', undefined, source);
    playQueue(got.tracks, 0, title ?? null);
    return true;
  }
  // The device's own (local mode, decision 0052): its album as the app lists it.
  const r = source === DEVICE ? deviceAlbumView(album) : await albumView(source, album);
  if (!r.rows.length) throw new Error('That album has no tracks.');
  playQueue(
    r.rows.map((x) => ({ source: r.source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: album, duration_secs: x.duration_secs })),
    0,
    title ?? r.title,
  );
  return true;
}

/** What plays where the output is now, to move it: the queue, the place in it and the position (s). */
const current = () => port().current();

/** Plays on `output` from now on, and moves what plays there: the queue from the playing track, at its position; the
 *  side that played stops. */
export async function moveTo(output: typeof prefs.output) {
  if (output === prefs.output) return;
  const was = current();
  await port().leave(was);
  pointAt(output);
  follow(output.startsWith('player:') ? output.slice('player:'.length) : null);
  if (!was) return;
  playQueue(was.tracks, was.index, null, was.position > 1 ? was.position : 0, false, true);
}

/** Whether music plays (or waits, paused) where the music plays now: what a move would take along. */
export const hasMusic = () => current() !== null;

/** Plays on `output` from now on without moving anything there: what plays there plays on, and what played here pauses
 *  (the owner chose not to take that player over). */
export async function joinTo(output: typeof prefs.output) {
  if (output === prefs.output) return;
  await port().pauseToJoin();
  pointAt(output);
  follow(output.startsWith('player:') ? output.slice('player:'.length) : null);
}

/** The screen follows the music (0.10; the owner, 2026-10-30: "Every screen showing that player"): moved from the
 *  player this screen shows to `name` (a handover elsewhere), it shows and drives that one, as if chosen in Play On;
 *  nothing is moved or paused by it. */
export function followTo(name: string) {
  if (drivenName() === name) return;
  pointAt(`player:${name}`);
  follow(name);
}
if (typeof window !== 'undefined') {
  $effect.root(() => {
    // A screen driving a player whose music went to another since it began to: after it (a move told before this
    // screen chose that player is not followed, so choosing it again stays the owner's choice).
    let since = 0;
    let shown: string | null = null;
    $effect(() => {
      const name = drivenName();
      if (name !== shown) {
        shown = name;
        since = Date.now();
      }
      if (!name) return;
      const moved = (playOn.players.find((p) => p.name === name) as { moved_to?: { player: string; at: number } } | undefined)?.moved_to;
      if (!moved || moved.player === name) return;
      const atMs = moved.at > 1e12 ? moved.at : moved.at * 1000;
      if (atMs + 2000 < since) return;
      untrack(() => followTo(moved.player));
    });
  });
}

/** Moves the music here. */
export const toLocal = () => moveTo('local');
