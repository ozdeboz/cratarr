// The player that plays, as the page drives it (0.10; the architecture audit, 2026-10-29: one Player port instead of
// a fork at each use). The page's own queue (this browser's or app's player, lib/localPlayer, which also drives one of
// Cratarr's players, decision 0031), or none where a browser cannot play and no player is chosen (0.12: the media
// server's player, its other side, went from Cratarr's mode). Which
// one is `port()`'s to say, here only (lib/player/commands' holdsQueue); everything else asks the port. What each side
// does is as it was where it was asked: the readings, the commands, what is shown before a command is confirmed.
import {
  playlistRows,
  setFavorite,
  setMember,
  setTrackFavorite,
  setTrackMember,
  trackState,
  type NowPlaying,
  type PlayHow,
  type PlayMode,
  type Playlist,
  type TrackState,
  type Transport,
} from '../api';
import { live } from '../live.svelte';
import {
  audioTap,
  canPlayHere,
  enqueue,
  jump as localJump,
  local,
  next as localNext,
  playQueue,
  previous as localPrevious,
  seek as localSeek,
  stop,
  togglePlay,
  windArm,
  windDisarm,
  windHold,
  windRelease,
  type LocalTrack,
} from '../localPlayer.svelte';
import { rewindNow } from '../ramping.svelte';
import { drivenName, outputName } from '../playOn.svelte';
import { prefs } from '../prefs.svelte';
import { toast } from '../toast.svelte';
import { holdsQueue } from './commands';

type Dir = 'back' | 'forward';
/** What plays, to move it elsewhere: the queue, the place in it and the position (s). */
export type Playing = { tracks: LocalTrack[]; index: number; position: number };

export type PlayerPort = {
  /** The page holds the queue: it plays here, or on one of Cratarr's players it drives. */
  holdsQueue: boolean;
  /** What plays, as a now-playing view. */
  playing(): NowPlaying | null;
  /** Actions on what plays wait (a command not confirmed yet, or the track's playlists and favourite being asked). */
  pending(): boolean;
  /** A command to the server's player is not confirmed yet: what plays may still turn out otherwise. */
  moving(): boolean;
  /** Its position where the page knows it itself (s); null: the remote's clock runs it on from the reports. */
  position(): number | null;
  /** A tape's rewind goes back on purpose: shown as it goes. */
  rewinding(): boolean;
  /** Start and stop's ramps run on this side (this page's player or its app's). */
  ramps: boolean;
  /** The queue as the page holds it (its tracks, the playing one's place, what it was started from); null where the
   *  server's player holds it. */
  queue(): { tracks: readonly LocalTrack[]; index: number; from: string | null } | null;
  /** Radio, Autoplay and Start radio (decision 0036): only where the page holds the queue. */
  radio: boolean;
  /** The sleep timer is this side's (the page's own, or handed to the player it drives). */
  sleeps: boolean;
  /** Level the volume runs on this side: what it turns the playing track down by (dB, 0 or less) and whether that is
   *  known yet; null where it does not. */
  level(): { db: number; known: boolean } | null;
  /** Its position can be dragged and seeked from here. */
  seekable: boolean;
  /** A Bandcamp preview plays on this side's own player (lib/bandcampFound). */
  previews: boolean;
  /** The sound as this page plays it, for the visuals to hear (null where it does not play here). */
  tap(): { context: AudioContext; node: AudioNode } | null;
  /** Play-pause, next or previous; `predict` shows what it leads to where the answer comes later. */
  send(command: Transport, predict: (command: Transport) => void): Promise<void> | void;
  /** Holding Previous or Next winds the tape (decision 0074). */
  wind(dir: Dir, phase: 'start' | 'stop'): void;
  /** A touch on Previous or Next that may become a hold: the wind asked already; dropped if it was a tap. */
  windTouch(dir: Dir, on: boolean): unknown;
  /** Anything can follow what plays. */
  hasNext(): boolean;
  /** Plays upcoming track `i`. */
  playUpcoming(i: number): Promise<void> | void;
  /** Seeks to `secs` (this side only; the server's player cannot be dragged, decision 0019). */
  seekTo(secs: number): unknown;
  /** Moves what plays `secs` on, from where it is shown (`at`, s), within its track (`end`, its length). */
  seekBy(secs: number, at: () => number, end: () => number | null): Promise<void> | void;
  /** The playing track's favourite set. */
  setFavorite(np: NowPlaying, on: boolean): Promise<void>;
  /** The playlists, with whether the playing track is in each. */
  playlists(): Playlist[];
  /** Whether `playlists` has been read (until then its [] is not known to be empty). */
  playlistsKnown(): boolean;
  /** The playing track put in a playlist or taken out. */
  setMember(playlistId: string, member: boolean): Promise<void>;
  /** A playlist played, shuffled, put next or added to the queue. */
  playPlaylist(id: string, mode: PlayMode): Promise<void>;
  /** These tracks put next or added to the queue (`source` theirs). */
  enqueue(tracks: LocalTrack[], how: 'next' | 'last', source: string): Promise<void> | void;
  /** One track put next or added (the server's player takes it by its id alone). */
  enqueueOne(track: LocalTrack, how: 'next' | 'last'): Promise<void> | void;
  /** One way to play tracks in hand (0.10, the audit: one way to play): played from `index`, shuffled, put next or
   *  added to the queue, as the queue named `name`. Here the page queues them; the server's player is given them by
   *  their ids (`source` theirs), the server queuing them itself, as a collection it picked. */
  playTracks(tracks: LocalTrack[], how: PlayHow, name: string | null, source: string, index?: number): Promise<void> | void;
  /** These tracks from `index` (a playlist's row tapped). */
  playFrom(name: string, source: string, tracks: LocalTrack[], index: number, position: number): Promise<void> | void;
  /** What plays, to move it. */
  current(): Playing | null;
  /** The side that played stops, its music moving elsewhere. */
  leave(was: Playing | null): Promise<void> | void;
  /** What plays here pauses, another player taken up as it plays. */
  pauseToJoin(): Promise<void> | void;
};

/** Whether the page holds the queue: it plays here, or on one of Cratarr's players it drives. A browser that cannot
 *  play here (no stream for audio) plays on a player, whatever was chosen elsewhere. */
export function isLocal() {
  return holdsQueue(prefs.output, canPlayHere, drivenName());
}

/** The player the page drives now. */
export function port(): PlayerPort {
  return isLocal() ? here : nowhere;
}

// ---- The page's own queue

/** What comes after the playing track, as the player shows it: made once per change of the queue or its place, not
 *  at every read (the tablet, 2026-10-28: every read of the player mapped the rest of a 273-track queue, many times a
 *  frame). */
const upcomingLocal = $derived(
  local.queue.slice(local.index + 1).map((u) => ({
    id: u.id,
    name: u.name || '…',
    artist: u.artist,
    album: u.album,
    album_id: u.album_id,
    addable: u.addable ?? true,
    duration_secs: u.duration_secs ?? null,
    lane: u.lane,
    ...(u.asked_by ? { asked_by: u.asked_by } : {}),
  })),
);
/** The local queue's playing track as a now-playing view (the shape the player shows), made once per change. */
const nowLocal = $derived.by((): NowPlaying | null => {
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
    progress: { position_secs: local.position, duration_secs: local.duration, paused: local.paused, starting: local.starting, estimated: false },
    player: outputName(),
    favorite: localState.view?.favorite ?? null,
    // As the heart, the chips stay until the new track's arrive (no row coming and going between tracks).
    info: localState.view?.info ?? null,
    upcoming: upcomingLocal,
  };
});
export const localNowPlaying = (): NowPlaying | null => nowLocal;

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

/** The local favourite changed: the answer, if it is still about the playing track. */
export function tookLocal(source: string, id: string, view: TrackState) {
  if (localState.key === `${source}/${id}`) localState.view = view;
}

const here: PlayerPort = {
  holdsQueue: true,
  playing: localNowPlaying,
  pending: () => localState.loading,
  moving: () => false,
  position: () => local.position,
  rewinding: () => rewindNow.on,
  ramps: true,
  queue: () => ({ tracks: local.queue, index: local.index, from: local.from }),
  radio: true,
  sleeps: true,
  level: () => ({ db: local.levelDb, known: local.levelKnown }),
  seekable: true,
  previews: true,
  tap: audioTap,
  send(command) {
    if (command === 'play-pause') togglePlay();
    else if (command === 'next') localNext();
    else if (command === 'previous') localPrevious();
  },
  wind: (dir, phase) => void (phase === 'start' ? windHold(dir) : windRelease(dir)),
  windTouch: (dir, on) => (on ? windArm(dir) : windDisarm()),
  hasNext: () => local.index < local.queue.length - 1 || !!local.autoplay || local.radio || local.repeat !== 'off',
  playUpcoming: (i) => localJump(local.index + 1 + i),
  seekTo: (secs) => localSeek(secs),
  seekBy(secs, at, end) {
    const length = end();
    localSeek(Math.max(0, Math.min(length ? length - 1 : Infinity, at() + secs)));
  },
  async setFavorite(np, on) {
    tookLocal(np.source, np.id, await setTrackFavorite(np.source, np.id, on));
  },
  playlists: () => localState.view?.playlists ?? (live.view?.playlists ?? []).map((p) => ({ ...p, member: false })),
  playlistsKnown: () => !!live.view || !!localState.view,
  async setMember(playlistId, member) {
    const t = local.queue[local.index];
    if (!t) throw new Error('Nothing is playing.');
    const view = await setTrackMember(t.source, t.id, playlistId, member);
    if (localState.key === keyOf(t)) localState.view = view;
  },
  async playPlaylist(id, mode) {
    // With their names: a player elsewhere (the app, a Cratarr player) shows them at once, on its lock screen too.
    const { source, rows } = await playlistRows(id, prefs.source || undefined);
    const named: LocalTrack[] = rows
      .filter((r) => r.id && !r.missing)
      .map((r) => ({ source, id: r.id!, name: r.name ?? '', artist: r.artist, album: r.album, album_id: null }));
    if (!named.length) throw new Error('None of its tracks can be found.');
    here.playTracks(named, mode, id, source);
  },
  playTracks(tracks, how, name, _source, index = 0) {
    if (how === 'next' || how === 'last') enqueue(tracks, how);
    else playQueue(tracks, index, name, 0, how === 'shuffle');
  },
  enqueue: (tracks, how) => enqueue(tracks, how),
  enqueueOne: (track, how) => enqueue([track], how),
  playFrom: (name, _source, tracks, index, position) => playQueue(tracks, index, name, position),
  current: () => (local.index < 0 ? null : { tracks: [...local.queue], index: local.index, position: local.position }),
  leave() {
    stop();
  },
  pauseToJoin() {
    if (local.index >= 0 && !local.paused) togglePlay(true);
  },
};

// ---- No player (0.12; the owner: the media server's players belong only in its own mode, 0.13)

/** A browser that cannot play here, with none of Cratarr's players chosen: nowhere to play until one is (Where it
 *  plays, lib/OutputSheet, lists them). Asked to play, it says so in one line; nothing else does anything. */
export const choosePlayer = () => toast.show({ text: "This browser can't play: choose a player in Where it plays." });

const nowhere: PlayerPort = {
  holdsQueue: false,
  playing: () => null,
  pending: () => false,
  moving: () => false,
  position: () => null,
  rewinding: () => false,
  ramps: false,
  queue: () => null,
  radio: false,
  sleeps: false,
  level: () => null,
  seekable: false,
  previews: false,
  tap: () => null,
  send: () => choosePlayer(),
  wind() {},
  windTouch: () => undefined,
  hasNext: () => false,
  playUpcoming() {},
  seekTo: () => undefined,
  seekBy() {},
  setFavorite: async () => {},
  playlists: () => [],
  playlistsKnown: () => true,
  setMember: async () => {},
  playPlaylist: async () => choosePlayer(),
  enqueue: () => choosePlayer(),
  enqueueOne: () => choosePlayer(),
  playTracks: () => choosePlayer(),
  playFrom: () => choosePlayer(),
  current: () => null,
  leave() {},
  pauseToJoin() {},
};
