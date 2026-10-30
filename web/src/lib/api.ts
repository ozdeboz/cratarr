import { track } from './progress.svelte';
import { keepForLater, later as laterWaits, laterKey, sendLater } from './later.svelte';
import { cue } from './queueCue.svelte';
import type { SearchResult } from './search';
import { coverStamps } from './coverStamps.svelte';
import { coverKey, coverRounds, localCovers } from './localCovers.svelte';
import { asksSignIn, notReached, reached, refused, refusedFor, toSignIn, unreached, keptAnswer } from './reach.svelte';
import type { LocalTrack } from './localPlayer.svelte';
import { asksWithin, curveBody, curveQuery, type CurveAsk } from './likeCurveAsk';

/** What a page says while Cratarr is out of reach (lib/reach.svelte.ts). */
export const UNREACHED = 'Cannot reach the Cratarr server';
export type Playlist = {
  id: string;
  name: string;
  member: boolean;
  /** Tracks that resolve to a file (what players show). */
  count: number;
  /** Entries that point at a missing file. */
  dead: number;
  /** Where it is kept in step (decision 0014). */
  links: Link[];
  /** Other names it is spoken as, for voice assistants (decision 0015). */
  spoken: string[];
  /** Whose it is (decision 0051), whether its owner shares it with the other users, whether it is the signed-in user's
   *  own (only its owner changes it). Left out by a server from before several users: one's own, unshared. */
  owner?: string;
  shared?: boolean;
  mine?: boolean;
};

/** A playlist's sync: "shared" (with everyone), or its copy in an account. */
export type Link = {
  /** "shared", or "user/<server>/<account>". */
  target: string;
  label: string;
  status: { at: number; error: string | null } | null;
  /** The account's copy exists (false while it waits to be made). */
  made: boolean;
};


export type NowPlaying = {
  /** The source it came from (covers are looked up there). */
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  /** The source's album id (used to mark the playing album on the Incoming page). */
  album_id: string | null;
  /** False when the track is outside the curated library, so it cannot be added to a playlist. */
  addable: boolean;
  /** Null when nothing reports a position. */
  progress: Progress | null;
  /** The player it plays on ("Finamp on Lenovo Tab"); the controls act on it. */
  player: string | null;
  /** Whether the player's account has it as a favourite; null when not known. */
  favorite: boolean | null;
  /** What plays next, in order, as far as the player reports. */
  upcoming: Upcoming[];
  /** What the server knows about the track's file and plays (the player's info chips, decision 0019). */
  info?: TrackInfo | null;
};

/** Each field null when the server does not know it. */
export type TrackInfo = {
  play_count: number | null;
  codec: string | null;
  bitrate_kbps: number | null;
  bit_depth: number | null;
  sample_rate_hz: number | null;
  explicit: boolean | null;
  /** Its tempo (decision 0035); absent from a server before it. */
  bpm?: number | null;
};

/** A track the player plays after the current one. */
export type Upcoming = {
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  addable: boolean;
  duration_secs: number | null;
  /** `next`: added by hand, ahead of the album or playlist (decision 0033's two lanes); `radio`: Radio's inserts;
   *  `autoplay`: Autoplay's, after everything (decision 0036, amended). */
  lane?: 'next' | 'radio' | 'autoplay';
  /** Who asked for it, a queue request accepted (0.10: a follower's name, or a guest's). */
  asked_by?: string;
};

export type Progress = {
  position_secs: number;
  /** Its player plays a tape's rewind (Tape rewind, the app): the position goes back as reported, not run on. */
  rewinding?: boolean;
  duration_secs: number | null;
  paused: boolean;
  /** Asked to play from rest and not sounding yet (`starting`, 2026-10-28): still, as the waveform shows it. */
  starting?: boolean;
  /** No player reports this track; the position is time since it started (wrong after a pause or seek). */
  estimated: boolean;
};

/** An enabled, connected source; `playing` is only filled by `getState`. */
export type SourceTab = { id: string; label: string; playing: boolean };

export type State = {
  now_playing: NowPlaying | null;
  playlists: Playlist[];
  sources: SourceTab[];
};

/** `generic`: no words from the server, only its status; callers say what failed instead (see toast.fail). `status`:
 *  the server's answer, where it gave one (a caller that tells a 404 from a 409 reads it). */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly generic = false,
    readonly status?: number,
  ) {
    super(message);
  }
}

/** A request the user is waiting for: counted for the progress bar (lib/progress.svelte.ts). */
function request<T>(path: string, init?: RequestInit): Promise<T> {
  return track(fetchJson<T>(path, init));
}

/** A request in the background (polling, checks while typing): no progress bar. */
function quietly<T>(path: string, init?: RequestInit): Promise<T> {
  // Not while a refusal holds them back (lib/reach): each would be one more knock on a ban.
  if (refusedFor() > 0) return Promise.reject(new ApiError(UNREACHED));
  return fetchJson<T>(path, init);
}

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    unreached();
    throw new ApiError(UNREACHED);
  }
  if (asksSignIn(res)) {
    toSignIn();
    throw new ApiError(UNREACHED);
  }
  if (!res.ok) {
    let message: string | null = null;
    try {
      const body = await res.json();
      if (typeof body.error === 'string') message = body.error;
    } catch {
      /* no words from the server */
    }
    if (!message && (res.status === 403 || res.status === 401)) refused();
    if (!message && notReached(res.status)) {
      unreached();
      throw new ApiError(UNREACHED);
    }
    reached();
    throw message ? new ApiError(message, false, res.status) : new ApiError(`Server returned ${res.status}`, true, res.status);
  }
  // Its last copy, the server out of reach (public/sw.js): shown, and said to be offline.
  if (res.headers.get('X-Cratarr-Kept')) keptAnswer();
  else reached();
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

/** A change that waits for Cratarr while it is out of reach (lib/later): tried now with its replay key; not reached,
 *  kept in order and sent when Cratarr answers again, `answer` standing for Cratarr's meanwhile. While others wait, it
 *  waits behind them. */
async function later<T>(path: string, init: RequestInit, answer: () => T): Promise<T> {
  const key = laterKey();
  const headers = new Headers(init.headers);
  headers.set('Idempotency-Key', key);
  const keep = () => {
    keepForLater({ key, method: init.method ?? 'POST', path, body: typeof init.body === 'string' ? init.body : null, type: headers.get('Content-Type') });
    return answer();
  };
  if (laterWaits.list.length) {
    const kept = keep();
    void sendLater();
    return kept;
  }
  try {
    return await fetchJson<T>(path, { ...init, headers });
  } catch (e) {
    if (e instanceof ApiError && e.message === UNREACHED) return keep();
    throw e;
  }
}
const laterJson = <T>(path: string, body: unknown, answer: () => T, method = 'POST') =>
  later<T>(path, { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, answer);

/** `source`: follow that source; absent: whichever started playing most recently. */
const follow = (source?: string) => (source ? `?source=${encodeURIComponent(source)}` : '');

export const getState = (source?: string) => quietly<State>(`/api/state${follow(source)}`);

// Cratarr's players' messages (Play On, decision 0031), as the server's own types say them (0.10: one protocol,
// generated from the Rust by ts-rs into lib/generated): a track as they hold it, a player's view, a device's audio
// output, a queue in its list and its link (decision 0045); what can be asked of one and what one reports.
export type { QueueTrack } from './generated/QueueTrack';
export type { PlayOnPlayer } from './generated/PlayOnPlayer';
export type { AudioOutput } from './generated/AudioOutput';
export type { QueueItem } from './generated/QueueItem';
export type { PlayerLink } from './generated/PlayerLink';
export type { PlayerCommand } from './generated/PlayerCommand';
export type { PlayerReport } from './generated/PlayerReport';
import type { PlayerCommand } from './generated/PlayerCommand';
import type { QueueTrack } from './generated/QueueTrack';
import type { PlayOnPlayer } from './generated/PlayOnPlayer';

export const getPlayers = () => quietly<{ players: PlayOnPlayer[] }>('/api/players');
export const getPlayer = (name: string) => quietly<PlayOnPlayer>(`/api/players/${encodeURIComponent(name)}`);

/** A row of Cratarr's diagnostics log (decision 0045): `at` and `last` epoch ms, `count` how often it came. */
export type LogRow = {
  id: number;
  at: number;
  last: number;
  count: number;
  level: 'error' | 'warn' | 'info' | 'debug' | 'trace';
  area: string;
  place: string;
  what: string;
  fields: Record<string, unknown>;
  origin: 'server' | 'device';
};
/** Which rows, newest first: a place (a player, or "server"), an area, the least serious level, the page before a row. */
export type LogQuery = { place?: string; area?: string; level?: string; before?: number; limit?: number };
export const getLog = (q: LogQuery) => {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) if (v !== undefined && v !== '') p.set(k, String(v));
  return quietly<{ rows: LogRow[] }>(`/api/diagnostics?${p}`);
};
/** What is logged by area now: the areas, the levels, each area raised for a while (`now`), until when (epoch ms). */
export type LogLevels = { areas: string[]; levels: string[]; now: Record<string, string>; until: number | null };
export const getLogLevels = () => quietly<LogLevels>('/api/diagnostics/levels');
export const setLogLevels = (areas: Record<string, string>, hours: number) =>
  request<LogLevels>('/api/diagnostics/levels', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ areas, hours }) });
/** A track's ReplayGain (decision 0035): gains in dB, peaks linear; what is not known is left out. */
/** `trackDown`/`albumDown` (0.9.8): the dB the player turns the track down by, track- or album-wise (0 or less;
 *  null where its gain is not known, the device's choice then applies). Older servers say only the gains and peaks. */
export type TrackGain = { trackGain?: number; albumGain?: number; trackPeak?: number; albumPeak?: number; trackDown?: number | null; albumDown?: number | null };
export const getTrackGain = (source: string, id: string) => quietly<TrackGain>(`/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}/gain`);
/** A Kodi Cratarr plays on: its web server, its login (the password never comes back), Cratarr's address for it. */
export type KodiSetting = { name: string; url: string; user: string; password?: string; password_set?: boolean; stream_from: string };
export type PlayOnSettings = { webhooks: string[]; kodi: KodiSetting[]; stream_default: string | null };
export const getPlayOnSettings = () => request<PlayOnSettings>('/api/players/settings');
export const savePlayOnSettings = (s: { webhooks: string[]; kodi: KodiSetting[] }) =>
  request<PlayOnSettings>('/api/players/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(s) });
/** A player's speaker delay (sync play, decision 0037), 0 to 500 ms. */
export const setPlayerDelay = (name: string, ms: number) =>
  request<{ ms: number }>(`/api/players/${encodeURIComponent(name)}/delay`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ms }),
  });
/** One's own device shared with everyone on this Cratarr, for Play On, or not (0.9.8). */
export const setPlayerShared = (name: string, shared: boolean) =>
  request<{ shared: boolean }>(`/api/players/${encodeURIComponent(name)}/shared`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ shared }),
  });
/** A command for one of Cratarr's players (the server's PlayerCommand). */
export const playerCommand = (name: string, command: PlayerCommand) =>
  request<void>(`/api/players/${encodeURIComponent(name)}/command`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(command),
  });

// Following and queue requests (0.10; the owner, 2026-10-30: "This is a queue matter"): another user's playing,
// listened along to on one's own player, its leader asked for tracks, who accepts or declines each (some people at
// once); and a guest's, on the home network (lib/guest).
const json = (method: string, body?: unknown): RequestInit => ({ method, headers: { 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
/** Someone who may be followed, their players and what each plays. */
export type FollowLeader = { user: string; players: { name: string; playing: boolean; track?: QueueTrack | null; position_ms: number; position_at: number }[] };
/** Someone following one of my players now (`players`), or one I accept at once who does not now (none). */
export type Follower = { user: string; players: string[] };
/** Who asks: a user, or a guest by the name they gave. */
export type Asker = { user: string } | { guest: string };
/** A track asked for, played next or at the end of `player`'s queue once accepted. */
export type QueueRequest = { id: string; from: Asker; track: QueueTrack; how: 'next' | 'last'; at: number; player: string; state?: 'pending' | 'accepted' | 'declined' };
/** How following and asking stand on a player (a server before 0.10 says none of it). */
export type FollowView = { following?: { user: string; player: string } | null; followers?: string[]; queue_requests?: number };
export const getLeaders = () => quietly<FollowLeader[]>('/api/follow/leaders');
export const getFollowers = () => quietly<Follower[]>('/api/follow/followers');
export const followOn = (mine: string, user: string, player: string) => request<void>(`/api/players/${encodeURIComponent(mine)}/follow`, json('PUT', { user, player }));
export const stopFollowing = (mine: string) => request<void>(`/api/players/${encodeURIComponent(mine)}/follow`, json('DELETE'));
export const getQueueRequests = (player?: string) => quietly<QueueRequest[]>(`/api/queue-requests${player ? `?${new URLSearchParams({ player })}` : ''}`);
export const askForTrack = (to: { user: string; player: string }, track: QueueTrack, how: 'next' | 'last') =>
  request<{ id: string; state: 'pending' | 'accepted' }>('/api/queue-requests', json('POST', { to, track, how }));
export const acceptRequest = (id: string) => request<void>(`/api/queue-requests/${encodeURIComponent(id)}/accept`, json('POST'));
export const declineRequest = (id: string) => request<void>(`/api/queue-requests/${encodeURIComponent(id)}/decline`, json('POST'));

/** A track's cover. The device's own music (local mode, decision 0052) has its album's, which the app serves on the
 *  page's own address. */
export const coverUrl = (np: { source: string; id: string; album_id?: string | null }) =>
  np.source === 'device' ? `/__device/cover/${encodeURIComponent(np.album_id ?? '')}` : `/api/cover/${encodeURIComponent(np.source)}/${encodeURIComponent(np.id)}`;

/** play: in file order · shuffle: random order · next: after the current track · last: at the end of the queue. */
export type PlayMode = 'play' | 'shuffle' | 'next' | 'last';
/** sent ≤ 240 of total playable tracks; unknown = lines left out (missing, or not known to the server yet). */
export type PlayResult = { player: string; sent: number; total: number; unknown: number };

export const playPlaylist = (playlistId: string, mode: PlayMode, source?: string) =>
  post<PlayResult>(`/api/playlists/${encodeURIComponent(playlistId)}/play${follow(source)}`, { mode }).then((r) => {
    if (mode === 'next' || mode === 'last') cue(mode);
    return r;
  });

// The queue (decision 0019): one track after the current one or at the end, playing on from an upcoming track, and a
// new playlist made of what plays and what comes next (tracks in the library only).
export const queueTrack = (id: string, mode: 'next' | 'last', source?: string) =>
  post<{ player: string }>(`/api/queue/add${follow(source)}`, { id, mode }).then((r) => (cue(mode), r));
/** Several tracks, in order (a crate's Play next / Add to queue, decision 0024). */
export const queueTracks = (ids: string[], mode: 'next' | 'last', source?: string) =>
  post<{ player: string }>(`/api/queue/add${follow(source)}`, { ids, mode }).then((r) => (cue(mode), r));

// What plays is the server's choice (server/src/app/play.rs): any collection, played, shuffled, put next or queued, at
// most 500 of its tracks, picked from all of it (the owner, 2026-10-23: a big crate's Shuffle played the same few, its
// panel's first page). lib/play.svelte.ts plays them where the output is.
export type Collection =
  | { kind: 'crate'; id: string } // genres are crates: id 'genre:<name>'
  | { kind: 'playlist'; id: string }
  | { kind: 'album'; source: string; id: string }
  | { kind: 'artist'; source: string; name: string }
  | { kind: 'tracks'; source: string; ids: string[] };
/** play: in its order (from a track, or its first 500) · shuffle: 500 from all of it, mixed · next: after the playing
 *  track · last: at the end of the queue. */
export type PlayHow = 'play' | 'shuffle' | 'next' | 'last';
/** A crate with its two toggles as the page shows them (a tap may be on its way); else as saved. `without`: the
 *  libraries switched off (CrateLook). */
export type Picking =
  | Collection
  | {
      kind: 'crate';
      id: string;
      favourites?: CrateFavourites;
      favourite_albums?: CrateFavourites;
      without?: string[];
      filter?: Condition[];
      order?: CrateOrder;
      down?: boolean;
    };
/** The tracks chosen, in queue order (`n`: each one's place in the collection, for shuffle off), the one that plays
 *  first, how many it has that can be played, and the player that took them when the server queued them. */
export type Picked = {
  source: string;
  tracks: LocalTrack[];
  index: number;
  total: number;
  player?: string;
};
/** The server picks what plays of `what`; with `on` it also queues them: on the server's player (`server`), or on one
 *  of Cratarr's players by name. `from`: the track (its id) to play first. */
export const pickCollection = (what: Picking, how: PlayHow, opts: { from?: string; on?: 'server' | { player: string }; source?: string } = {}) =>
  post<Picked>(`/api/play${follow(opts.source)}`, { what, how, ...(opts.from ? { from: opts.from } : {}), ...(opts.on ? { on: opts.on } : {}) }).then((r) => {
    // Queued by the server (picked only, this browser queues them itself: lib/localPlayer's enqueue tells it).
    if (opts.on && (how === 'next' || how === 'last')) cue(how);
    return r;
  });

/** A crate (decision 0024): a permanent one or one's own; `count` null when not known (the server's favourites could
 *  not be read). */
/** `favourites`: its toggle's setting; `counts`: its size with each setting (null: the favourites are not known). */
export type Crate = {
  id: string;
  name: string;
  favourites: CrateFavourites;
  /** Its favourite albums toggle (decision 0024, amended 2026-10-19); a server from before it: all. */
  favourite_albums?: CrateFavourites;
  /** Its size with each favourites setting, its favourite albums toggle as it is set. */
  counts: Record<CrateFavourites, number | null>;
  permanent: boolean;
  /** What its card shows, as its favourites toggle is set; absent where that is not known (favourites not read). */
  stats?: CrateStats;
  /** A picture of its own was set (crateImageUrl). */
  image: boolean;
  /** How many tracks it leaves out by hand (decision 0024, amended 2026-10-19); a server from before: none. */
  left_out?: number;
};
/** A crate's card (decided 2026-10-16): its size and length, how much was never played (null: no owner account to ask
 *  about), what came this week, its top three genres and up to four of its albums (most of its tracks first). */
export type CrateStats = {
  tracks: number;
  seconds: number;
  unplayed: number | null;
  new_week: number;
  genres: string[];
  covers: { source: string; album_id: string }[];
};

/** One condition of a crate: an axis, what it is tested against, negated or not. A filter's (decision 0061) can be
 *  muted (`enabled` false: ignored) and, on items that hold tracks (albums, playlists), hold for `any` of their tracks
 *  (the default) or `all` of those that have a value on the axis. Left out: on, any. */
export type CrateValue =
  | { kind: 'yes' }
  | { kind: 'any_of'; values: string[] }
  | { kind: 'range'; min: number | null; max: number | null }
  // Sounds like its seeds (song ids; 0.11 albums' and artists', `al-…`, `ar-…`): the `count` tracks that sound most
  // like them, at least `min` alike (0–100, the curve's scale), an artist seed's own tracks out unless `own`.
  | { kind: 'like'; of: string[]; count: number; min?: number; own?: boolean; fresh?: boolean; weights?: number[] };
export type Quantifier = 'any' | 'all';
export type Condition = { axis: string; not: boolean; value: CrateValue; enabled?: boolean; quantifier?: Quantifier };
/** `not_artists`: the artists left out of it by hand (0.10, decision 0083: "Not in this crate", an Artist-is-not row of
 *  its filter); none from a server before it. */
export type CrateDef = { id: string; name: string; conditions: Condition[]; not_artists?: ExcludedArtist[] };
/** An axis the builder draws a control for, with its domain in this library. */
export type Axis = {
  key: string;
  label: string;
  kind: 'yes_no' | 'values' | 'membership' | 'range' | 'like';
  values: string[];
  /** How many items of the list have each of `values` (the Filter sheet's, filterList); absent in the builder's. */
  counts?: number[];
  min: number | null;
  max: number | null;
  known: number;
  /** A range's density: how many tracks in each equal step from `min` to `max` (the sliders' bar). */
  bins?: number[];
  /** Whether an item's tracks differ on it, so any or all of them can mean different things (not an album's year). */
  varies?: boolean;
};
/** The libraries a crate leaves out (decision 0024, amended 2026-10-23: a crate draws from every library switched
 *  on): `source/library` keys, as the page's library switches keep them (prefs.hiddenLibraries). None, or left out:
 *  every library. */
/** `filter`: the filter on a crate (decision 0061), conditions besides its own (its rows, tempo spread, Play).
 *  `order`, `down`: the order its list shows (decision 0060), applied on the server before the rows are paged (and
 *  by Play); a value a row does not have comes first, last turned round; equals keep the crate's own order. */
export type CrateLook = { without?: string[]; filter?: Condition[]; order?: CrateOrder; down?: boolean };
/** A crate's list's orders: its own (newest first), or by a field. */
export type CrateOrder = 'own' | 'title' | 'artist' | 'albumArtist' | 'album' | 'year' | 'track' | 'length' | 'tempo' | 'plays' | 'lastPlayed' | 'added';
/** `?source=…&without=…&filter=…&order=…&down=true` (each key once; the filter as JSON). */
function crateQuery(q: URLSearchParams, source?: string, look: CrateLook = {}) {
  if (source) q.set('source', source);
  for (const k of look.without ?? []) q.append('without', k);
  if (look.filter?.length) q.set('filter', JSON.stringify(look.filter));
  if (look.order && look.order !== 'own') q.set('order', look.order);
  if (look.down) q.set('down', 'true');
  const t = q.toString();
  return t ? `?${t}` : '';
}
export const getCrateAxes = (source?: string, without?: string[]) => quietly<Axis[]>(`/api/crates/axes${crateQuery(new URLSearchParams(), source, { without })}`);
export const getCrate = (id: string) => quietly<CrateDef>(`/api/crates/${encodeURIComponent(id)}`);
/** An artist left out (0.10, decision 0083): by the server's key (the name loosely) and as named. */
export type ExcludedArtist = { key: string; name: string; at?: number };
/** An artist left out of one crate, or taken back: the crate's list after it, and the undo. */
export const notInCrate = (id: string, name: string) =>
  request<{ crate: string; not_artists: ExcludedArtist[]; undo: string | null }>(`/api/crates/${encodeURIComponent(id)}/not-artist`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
export const backInCrate = (id: string, key: string) =>
  request<{ crate: string; not_artists: ExcludedArtist[]; undo: string | null }>(`/api/crates/${encodeURIComponent(id)}/not-artist/${encodeURIComponent(key)}`, { method: 'DELETE' });
/** The artists the account excluded everywhere (every crate, radio, Autoplay, Discover), and the changes, each with its undo. */
export const getExcludedArtists = () => quietly<ExcludedArtist[]>('/api/me/excluded-artists');
export const excludeArtist = (key: string, name: string) =>
  request<ExcludedArtist & { undo: string | null }>(`/api/me/excluded-artists/${encodeURIComponent(key)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
export const includeArtist = (key: string) => request<{ key: string; undo: string | null }>(`/api/me/excluded-artists/${encodeURIComponent(key)}`, { method: 'DELETE' });
/** Every change answers with its undo token (`undo`, decision 0026), for `withUndo`. */
type Undo = { undo: string | null };
export const createCrate = (name: string, conditions: Condition[]) => post<{ id: string } & Undo>('/api/crates', { name, conditions });
export const updateCrate = (id: string, name: string, conditions: Condition[]) =>
  request<{ id: string } & Undo>(`/api/crates/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, conditions }),
  });
export const deleteCrate = (id: string) => request<{ deleted: string } & Undo>(`/api/crates/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** The builder's live preview: how many tracks unsaved conditions select, and the first of them. */
export const previewCrate = (conditions: Condition[], limit: number, source?: string, look: Omit<CrateLook, 'filter'> = {}) => {
  const q = new URLSearchParams({ limit: String(limit) });
  if (source) q.set('source', source);
  if (look.order && look.order !== 'own') q.set('order', look.order);
  if (look.down) q.set('down', 'true');
  return quietly<{ source: string; total: number; rows: PlaylistRow[] }>(`/api/crates/preview?${q}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ conditions, ...(look.without?.length ? { without: look.without } : {}) }),
  });
};
/** The list in view a filter is on (decision 0061): the Library's albums (on `source`) or its artists, the playlists,
 *  a playlist's, a crate's, an album's or an artist's tracks. */
export type FilterList =
  | { kind: 'library' }
  | { kind: 'artists' }
  | { kind: 'playlists' }
  | { kind: 'playlist'; id: string }
  | { kind: 'crate'; id: string; favourites?: CrateFavourites; favourite_albums?: CrateFavourites }
  | { kind: 'album'; source: string; id: string }
  | { kind: 'artist'; source: string; name: string };
/** What passes: `keys` of the items that do (an album's id, an artist's `source\0name`, a playlist's id, a track row's
 *  `entry`; empty for a crate, whose rows come filtered: CrateLook.filter), how many of how many, and the list's axes
 *  (values with `counts`, most common first; ranges' span) and tempo spread, over all its items. */
export type Filtered = {
  source: string;
  items: 'albums' | 'artists' | 'playlists' | 'tracks';
  keys: string[];
  passed: number;
  total: number;
  axes: Axis[];
  tempo: TempoBands;
  /** A list of tracks' profile (lib/ListProfile); none for albums and playlists. */
  profile?: ListProfileData | null;
};
/** What a list's tracks are like (the server's crates::Profile): telling style tags and artists, each number's density
 *  on one scale with its middle half and middle (a number all share: that value), and CLAP's moods against the
 *  library's (-1 to 1). */
export type ListProfileData = {
  tracks: number;
  heard: number;
  styles: string[];
  artists: string[];
  /** `one`: every track has this value (an album's year): shown as the value, no graph. */
  numbers: { key: 'tempo' | 'energy' | 'danceability' | 'year'; lo: number; hi: number; bins: number[]; q1: number; mid: number; q3: number; one?: number }[];
  moods: { name: string; lift: number }[];
};
/** A filter on the list in view: its conditions and its own switches that are on (lib/pageView SWITCHES: the server
 *  holds what each means, the owner, 2026-10-29: one filter, one engine); `without`, the libraries switched off (the
 *  Library's albums and artists, and a crate's tracks). */
export const filterList = (list: FilterList, conditions: Condition[], source?: string, without?: string[], switches?: string[]) =>
  quietly<Filtered>(`/api/filter${follow(source)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ list, conditions, ...(switches?.length ? { switches } : {}), ...(without?.length ? { without } : {}) }),
  });
export const getCrates = (source?: string, without?: string[]) => quietly<Crate[]>(`/api/crates${crateQuery(new URLSearchParams(), source, { without })}`);
/** A page of a crate's rows (newest first) and the crate's size. */
/** A crate's own picture (kept as JPEG within 800 px); `round` reads it again after a change. */
export const crateImageUrl = (id: string, round = 0) => `/api/crates/${encodeURIComponent(id)}/image${round ? `?v=${round}` : ''}`;
/** Sets it: any JPEG, PNG, GIF or WebP, up to 32 MB. */
export const setCrateImage = (id: string, file: Blob) =>
  request<void>(`/api/crates/${encodeURIComponent(id)}/image`, { method: 'PUT', headers: { 'Content-Type': file.type || 'application/octet-stream' }, body: file });
export const removeCrateImage = (id: string) => request<void>(`/api/crates/${encodeURIComponent(id)}/image`, { method: 'DELETE' });
/** The crate panel's favourites toggle (decision 0024, amended): all its tracks, only the favourites, or none. */
export type CrateFavourites = 'all' | 'only' | 'none';
/** `favourites`, `favouriteAlbums`: the toggles' settings as the page shows them (a tap may be on its way); the saved
 *  ones without. */
export const crateRows = (
  id: string,
  offset: number,
  limit: number,
  source?: string,
  favourites?: CrateFavourites,
  favouriteAlbums?: CrateFavourites,
  look: CrateLook = {},
) => {
  const q = new URLSearchParams({ offset: String(offset), limit: String(limit) });
  if (favourites) q.set('favourites', favourites);
  if (favouriteAlbums) q.set('favourite_albums', favouriteAlbums);
  // `limit`: the most rows the page could hold (the server serves 500 at most a page); a page with fewer is the last.
  return quietly<{ source: string; total: number; limit?: number; rows: PlaylistRow[]; genres: string[] }>(
    `/api/crates/${encodeURIComponent(id)}/rows${crateQuery(q, source, look)}`,
  );
};
/** A crate's tempo spread (decision 0041): its tracks' 5-BPM bands, 60 to 180, counted on the server. */
export type TempoBands = { bands: number[]; with_tempo: number; total: number };
/** How alike the tracks that sound most like these seeds are, closest first, 0 to 100 (lib/LikeCurve), and the profile
 *  of the closest `count` at every step its handle can stop at (the profile follows a drag). */
export type LikeCurveData = { alike: number[]; profiles: { count: number; profile: ListProfileData }[] };
/** Within a crate's other conditions (lib/likeCurveAsk, 0.11), POSTed; a server before 0.11 has only the GET, and
 *  is asked that once it said so (the curve then counts the whole library, as before). */
let curveWithin = true;
export const likeCurve = (ask: CurveAsk, source?: string): Promise<LikeCurveData> => {
  const get = () => quietly<LikeCurveData>(`/api/like-curve?${curveQuery(ask, source)}`);
  if (!curveWithin || !asksWithin(ask)) return get();
  return quietly<LikeCurveData>(`/api/like-curve${source ? `?${new URLSearchParams({ source })}` : ''}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(curveBody(ask)),
  }).catch((e) => {
    if (!missingCall(e)) throw e;
    curveWithin = false;
    return get();
  });
};
export const crateTempo = (id: string, source?: string, favourites?: CrateFavourites, favouriteAlbums?: CrateFavourites, look: CrateLook = {}) => {
  const q = new URLSearchParams();
  if (favourites) q.set('favourites', favourites);
  if (favouriteAlbums) q.set('favourite_albums', favouriteAlbums);
  return quietly<TempoBands>(`/api/crates/${encodeURIComponent(id)}/tempo${crateQuery(q, source, look)}`);
};
/** A playlist's facts for the Playlists page's table (decision 0042): its length, top genres, how much the owner never
 *  played (null: no owner account) and when they last played any of it, and when its file last changed (epoch s). */
export type PlaylistStats = { seconds: number; genres: string[]; unplayed: number | null; last_played: number | null; changed: number };
export const playlistStats = () => quietly<Record<string, PlaylistStats>>('/api/playlists/stats');
/** Make playlist: the crate's tracks now, as its favourites toggle is set, as a new ordinary playlist (a snapshot). */
export const crateToPlaylist = (id: string, name: string, source?: string, without?: string[]) =>
  post<{ playlist: string; added: number } & Undo>(`/api/crates/${encodeURIComponent(id)}/playlist${follow(source)}`, {
    name,
    ...(without?.length ? { without } : {}),
  });
/** A crate's favourites toggle, kept on the server for every device. */
export const setCrateFavourites = (id: string, favourites: CrateFavourites) =>
  request<{ crate: string; favourites: CrateFavourites }>(`/api/crates/${encodeURIComponent(id)}/favourites`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ favourites }),
  });
/** A crate's favourite albums toggle (decision 0024, amended 2026-10-19), kept on the server for every device. */
export const setCrateFavouriteAlbums = (id: string, favourite_albums: CrateFavourites) =>
  request<{ crate: string; favourite_albums: CrateFavourites }>(`/api/crates/${encodeURIComponent(id)}/favourite-albums`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ favourite_albums }),
  });
/** What is excluded (decision 0024, amended 2026-10-19): tracks (`ids`) and, from a crate, whole albums (`albums`,
 *  their tracks now and later). */
export type Excluding = { ids?: string[]; albums?: string[] };
/** What a ranked crate learned not to suggest from what was turned down for it (the owner, 2026-10-23: playlist fit
 *  models): AudioMuse styles, artists, CLAP's moods, a tempo band, an energy band (0 to 100). */
export type Trait = { kind: 'style' | 'artist' | 'mood'; name: string } | { kind: 'tempo' | 'energy'; lo: number; hi: number };
export function traitLabel(t: Trait): string {
  if ('name' in t) return t.kind === 'mood' ? t.name.charAt(0).toUpperCase() + t.name.slice(1) : t.name;
  if (t.kind === 'tempo') return `${Math.round(t.lo)}–${Math.round(t.hi)} BPM`;
  return t.hi >= 100 ? 'Energetic' : 'Calm';
}
/** Its fit, as its Not for sheet shows it: every trait learned (whether dismissed), one's own rules (strict), and how
 *  many were turned down. */
export type CrateFit = { learned: { trait: Trait; key: string; dismissed: boolean }[]; rules: Condition[]; turned_down: number };
export const getCrateFit = (id: string) => quietly<CrateFit>(`/api/crates/${encodeURIComponent(id)}/fit`);
/** Every ranked crate whose fit holds anything, the account's (Settings' Musical taste): its id and name (`of`: what
 *  it sounds like, its Not for sheet's name, "Cobalt Hour"), what it
 *  avoids (learned, not dismissed), its rules, how many were turned down. Each opens its Not for sheet. */
export type FitSummary = { crate: string; name: string; of: string; avoiding: Trait[]; rules: Condition[]; turned_down: number };
export const getCrateFits = () => quietly<FitSummary[]>('/api/crates/fits');
export const setCrateFit = (id: string, fit: { rules: Condition[]; dismissed: string[] }) =>
  request<void>(`/api/crates/${encodeURIComponent(id)}/fit`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(fit) });
/** A crate's exclusions: it leaves them out whatever its conditions and toggles say. Out and back in again are
 *  journaled (Undo); ids the library does not know are passed over. */
export const leaveOutOfCrate = (id: string, source: string, what: Excluding) =>
  post<{ left_out: number } & Undo>(`/api/crates/${encodeURIComponent(id)}/left-out`, { source, ...what });
export const putBackInCrate = (id: string, source: string, what: Excluding) =>
  request<{ left_out: number } & Undo>(`/api/crates/${encodeURIComponent(id)}/left-out`, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source, ...what }),
  });
/** An excluded album, as a crate's list shows it; no title for one the library no longer has. */
export type ExcludedAlbum = { id: string; title: string | null; artist: string | null };
export const crateLeftOut = (id: string, source?: string) =>
  quietly<{ source: string; rows: PlaylistRow[]; albums?: ExcludedAlbum[] }>(`/api/crates/${encodeURIComponent(id)}/left-out${follow(source)}`);
/** Global exclusions (the owner: "every automatic pick"): out of every crate, and never picked by Radio, Autoplay or a
 *  shuffle of the library or the favourites; still played when chosen. Journaled as a crate's are. */
export const excludeTracks = (source: string, ids: string[]) =>
  laterJson<{ excluded: number } & Undo>('/api/excluded', { source, ids }, () => ({ excluded: ids.length, undo: null }));
export const includeTracks = (source: string, ids: string[]) =>
  laterJson<{ excluded: number } & Undo>('/api/excluded', { source, ids }, () => ({ excluded: 0, undo: null }), 'DELETE');
export const excludedRows = (source?: string) => quietly<{ source: string; excluded: number; rows: PlaylistRow[] }>(`/api/excluded${follow(source)}`);
export const jumpTo = (id: string, source?: string) => post<{ player: string }>(`/api/queue/jump${follow(source)}`, { id });
/** `ids`: the queue to make it from (the local player's); without, the server's player's queue. `repeats`: a track
 *  the queue plays twice is in it twice (the page asked), else once (decision 0025, amended 2026-10-16). */
export const playlistFromQueue = (name: string, source?: string, ids?: string[], repeats = false) =>
  post<{ playlist: string; added: number; left_out: number } & Undo>(`/api/queue/playlist${follow(source)}`, ids ? { name, ids, repeats } : { name, repeats });

/** Mark the playing track a favourite or not. Says the state wanted, so a retried request changes nothing more. */
export const setFavorite = (on: boolean, source?: string) => {
  const q = new URLSearchParams({ on: String(on) });
  if (source) q.set('source', source);
  return quietly<State>(`/api/favorite?${q}`, { method: 'POST' }); // shown at once (optimistic)
};

/** Put the playing track in the playlist or take it out. Says the state wanted, so a retried request changes nothing more. */
export const setMember = (playlistId: string, member: boolean, source?: string) => {
  const q = new URLSearchParams({ member: String(member) });
  if (source) q.set('source', source);
  return quietly<State>(`/api/playlists/${encodeURIComponent(playlistId)}/toggle?${q}`, { method: 'POST' }); // shown at once
};

export type Repair = { playlist: string; from: string; to: string };
export type Repairs = { safe: Repair[]; review: Repair[] };

export const getRepairs = () => request<Repairs>('/api/repairs');

const post = <T>(path: string, body: unknown) =>
  request<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

/** Adjust BPM (an administrator's, the library's for everyone): a track's BPM now (its tag, else Cratarr's measure)
 *  and Cratarr's measure. */
export type TrackTempo = { id: string; bpm: number | null; measured: number | null };
export type TempoAdjust = 'half' | 'double' | 'reset';
export const getTempo = (source: string, ids: string[]) => post<{ tracks: TrackTempo[] }>('/api/tempo', { source, ids });
/** Halves, doubles or resets the tracks' BPM, written into their files; tracks with nothing to go from stay. */
export const adjustTempo = (source: string, ids: string[], adjust: TempoAdjust) =>
  request<{ tracks: TrackTempo[]; undo: string | null }>('/api/tempo', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source, ids, adjust }),
  });

/** The radio's next tracks (decision 0036), already in the radio's lane: like `seed` (the radio's seed, or the queue's
 *  own album or playlist), none of `exclude` (the queue), after `last` (the playing track). */
export const radioNext = (ask: {
  mode: string;
  seed: string[];
  exclude: string[];
  last: string | null;
  count: number;
  within?: string | null;
  albums?: boolean;
  /** Radio's inserts: what they are like (the track, its artist, its era). */
  by?: string;
  /** Leave the owner's favourites out. */
  skip_favourites?: boolean;
}) =>
  post<{ tracks: QueueTrack[] }>('/api/radio/next', ask).then(({ tracks }) => ({
    // Bandcamp's (Discover's suggestions, 0.9.5): previews, never in a playlist (decision 0071).
    tracks: tracks.map((t) => (t.source === 'bandcamp' ? { ...t, addable: false } : t)),
  }));

/** Apply every safe repair, or only those of these playlists (the panel's Repair tab, decision 0025). */
export const applySafe = (playlists?: string[]) =>
  post<{ playlists: string[] } & Undo>('/api/repairs/apply', playlists ? { kind: 'safe', playlists } : { kind: 'safe' });

/** Apply one offered repair (used for the ones that need a look). */
export const applyOne = (r: Repair) => post<{ playlists: string[] } & Undo>('/api/repairs/apply', { kind: 'one', ...r });
/** A missing line pointed at a track found for it (Health's missing tracks): no repair need offer it; the server
 *  checks the line is dead in the playlist and the track is a file in Music. Journaled as a repair. */
export const relinkLine = (r: Repair) => post<{ playlists: string[] } & Undo>('/api/repairs/relink', r);

/** The one undo (decision 0026): an action's tokens, undone newest first; the redo's tokens come back (none for
 *  Incoming's, whose albums' titles come back instead). */
export const undo = (tokens: string[]) => post<{ redo: string[]; restored: string[] }>('/api/undo', { tokens });


export const createPlaylist = (name: string) => post<{ playlist: string } & Undo>('/api/playlists', { name });

/** What to export (decision 0007): playlists by name, with their tracks or not, the tracks in one folder or the
 *  library's, and the playlists as they are or combined into one of this name. */
export type ExportChoice = { playlists: string[]; tracks: boolean; flat: boolean; combine: string | null };
/** What Download… gives as files (decision 0058, lib/ExportStep): playlists by id, or a list of tracks with a name (an
 *  album, an artist's, a crate's, a selection, a queue). Never Bandcamp's previews (decision 0071): the server refuses
 *  them. */
export type ExportWhat =
  | { kind: 'playlists'; names: string[] }
  | { kind: 'tracks'; name: string; tracks: { source: string; id: string }[] };
/** The files as they are; hi-res ones at CD quality (16-bit FLAC, 44.1 kHz); lossless ones as Opus, or as MP3 at 320
 *  kb/s or V0 (0.10; lossy ones always stay as they are). */
export type ExportFormat = 'original' | 'flac-cd' | 'opus' | 'mp3-320' | 'mp3-v0';
/** One folder, or Artist/Album folders. */
export type ExportLayout = 'flat' | 'folders';
/** An export (decision 0058): what, its format (Opus at `bitrate` kb/s: 96, 128, 160, 256) and layout; playlists with
 *  their tracks or only the playlist files (`tracks`), as they are or combined into one of this name; tracks with a
 *  playlist file of them or not (`m3u`). */
export type ExportRequest = {
  what: ExportWhat;
  format: ExportFormat;
  bitrate: number;
  layout: ExportLayout;
  tracks: boolean;
  combine: string | null;
  m3u: boolean;
};
/** What an export will hold, before starting: `bytes` only with tracks, `estimate` when converted files make it a
 *  guess; missing tracks are left out. */
export type ExportPreview = { tracks: number; missing: number; bytes: number | null; estimate: boolean };
export const exportPreview = (c: ExportChoice | ExportRequest) =>
  quietly<ExportPreview>('/api/export/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(c) });
/** Readies an export on the server for a few minutes; its download is then a plain link (`exportDownloadUrl`): one
 *  file as itself, several as one zip (0.10; the owner, 2026-10-30: "Multiple files should always be a zip. A single
 *  file, always a single file, not a zip"), `name` its name either way. */
export const prepareExport = (r: ExportRequest) =>
  request<{ token: string; name: string }>('/api/export', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(r) });
export const exportDownloadUrl = (token: string) => `/api/export/${encodeURIComponent(token)}`;
/** The download itself: a link, so the browser shows its progress and can cancel it. */
export function exportUrl(c: ExportChoice): string {
  const q = new URLSearchParams();
  for (const p of c.playlists) q.append('playlist', p);
  if (c.tracks) q.set('tracks', '1');
  if (c.tracks && c.flat) q.set('flat', '1');
  if (c.combine !== null) q.set('combine', c.combine);
  return `/api/export?${q}`;
}

/** A playlist's first albums (up to four, in playlist order), for the mosaic in its menu (decision 0021). */
/** One line of a playlist for its view (decision 0023): what the catalogue knows of its file; `id` null when not matched
 *  to the server, `missing` when the file is not in the Music folder. */
export type PlaylistRow = {
  entry: string;
  file: string;
  id: string | null;
  name: string | null;
  artist: string | null;
  album: string | null;
  duration_secs: number | null;
  missing: boolean;
  /** A playlist's rows, for the panel's orders: from the file's tags, */
  album_artist?: string;
  disc?: number;
  track?: number;
  year?: number;
  /** its tempo (the BPM tag, else the analysis', decision 0035), */
  bpm?: number;
  /** the user's plays of the track and when it was last played (epoch seconds), */
  play_count?: number;
  last_played?: number;
  /** and when the entry came (none for one held before Cratarr kept that; a crate's row: when its track came into the library). */
  added?: number;
  /** Its album's id, where the rows are of several albums (an artist's). */
  album_id?: string;
  /** A crate's row: how many tracks its album has (0.10; Cratarr's own library's rows). */
  album_tracks?: number;
  /** An album's row on Cratarr's own library: the library holds other recordings of its work (0.10, Other versions). */
  has_versions?: boolean;
};

/** Take a playlist's repeats out, the first copy of each track staying (decision 0025, amended 2026-10-16). */
export const removeRepeats = (id: string) =>
  request<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/repeats`, { method: 'DELETE' });

/** Add tracks (ids on `source`, in order) at a playlist's end, whether or not it holds them already: Add again. */
export const appendTracks = (id: string, source: string, ids: string[]) =>
  laterJson<{ playlist: string; added: number } & Undo>(`/api/playlists/${encodeURIComponent(id)}/append`, { source, ids }, () => ({
    playlist: id,
    added: ids.length,
    undo: null,
  }));

export const playlistRows = (id: string, source?: string) =>
  quietly<{ source: string; rows: PlaylistRow[] }>(
    `/api/playlists/${encodeURIComponent(id)}/rows${source ? `?source=${encodeURIComponent(source)}` : ''}`,
  );

/** Saves a playlist edited in its view (decision 0023): its lines reordered or some left out (`entries`), replacing
 *  `expected` (the lines as the view showed them); refused when it changed since. */
export const editPlaylist = (id: string, expected: string[], entries: string[]) =>
  request<{ playlist: string; entries: number } & Undo>(`/api/playlists/${encodeURIComponent(id)}/entries`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ expected, entries }),
  });

/** Lists like a list, of its kind (the one similarity): the closest first, each with how alike (0 to 100). */
export type Alike = { id: string; alike: number };
export const similarAlbums = (source: string, album: string) =>
  quietly<Alike[]>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/similar`);
export const similarPlaylists = (id: string) => quietly<Alike[]>(`/api/playlists/${encodeURIComponent(id)}/similar`);
export const similarCrates = (id: string) => quietly<Alike[]>(`/api/crates/${encodeURIComponent(id)}/similar`);
export const playlistCovers = (id: string) =>
  quietly<{ source: string; album_id: string }[]>(`/api/playlists/${encodeURIComponent(id)}/covers`);
export const renamePlaylist = (id: string, to: string) =>
  laterJson<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/rename`, { to }, () => ({ playlist: to, undo: null }));
export const deletePlaylist = (id: string) =>
  request<{ deleted: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** Shares one's own playlist with the other users, or stops (decision 0051): they see it, play it and copy it. */
export const setPlaylistShared = (id: string, shared: boolean) =>
  request<void>(`/api/playlists/${encodeURIComponent(id)}/shared`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ shared }) });
/** Another user's shared playlist (its id "owner/name"), copied into one's own under a free name: that name. */
export const copyPlaylist = (id: string) => post<{ playlist: string } & Undo>(`/api/playlists/${encodeURIComponent(id)}/copy`, {});

/** The other names a playlist is spoken as, exactly these. */
/** The names a playlist is also spoken as, from what was typed (comma-separated). */
export const spokenNames = (text: string) =>
  text
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

export const setSpoken = (id: string, names: string[]) =>
  request<Undo>(`/api/playlists/${encodeURIComponent(id)}/spoken`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ names }),
  });
const linkPath = (id: string, target: string, extra: Record<string, string> = {}) =>
  `/api/playlists/${encodeURIComponent(id)}/link?${new URLSearchParams({ target, ...extra })}`;
/** Link a playlist to a target, or change how it is kept (the state wanted: safe to repeat). */
export const linkPlaylist = (id: string, target: string) => request<null>(linkPath(id, target), { method: 'PUT' });
/** Take a link off; its copy on the target is deleted. */
export const unlinkPlaylist = (id: string, target: string) => request<null>(linkPath(id, target), { method: 'DELETE' });

export type Status = {
  connected: boolean;
  /** The server's name with one source; "n of m sources" with several. */
  server_name: string | null;
  /** A Music library is set and its folder found: playlists can be read and made. */
  music: boolean;
  /** Some enabled server has an Incoming library. */
  incoming: boolean;
  sources: SourceTab[];
  version: string;
  /** What is wrong with the servers, one line each (a server that stopped processing changes). */
  problems: string[];
};

export const getStatus = () => quietly<Status>('/api/status');

export type FieldKind = 'url' | 'text' | 'secret' | 'choice' | 'multi_choice';

/** A setting a source plugin asks for; the Settings page draws its form from these. */
export type Field = {
  key: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  help: string | null;
  /** Secrets: changing one of these fields needs the secret entered again. */
  bound_to: string[];
};

export type Choice = { value: string; label: string };

export type SourceStatus = {
  connected: boolean;
  server_name: string | null;
  version: string | null;
  detail: string | null;
  error: string | null;
};

export type Values = Record<string, string | string[]>;

/** What is saved for a source. Secrets left blank keep the saved ones (while their bound fields stay the same). */
/** What a server's libraries are used for, by library id (decision 0009). Libraries in neither list are not used. */
/** A library's own settings (decision 0034): what a move into it does to the files, and its dot's colour. */
export type LibraryPolicy = { format?: 'cd' | 'as_is'; colour?: string | null };
export type Roles = { music: string[]; incoming: string[]; policies?: Record<string, LibraryPolicy> };

export type LibraryUse = 'music' | 'incoming' | 'unused';

/** A server's music library and where Cratarr sees its files. */
export type ServerLibrary = {
  id: string;
  name: string;
  /** Cratarr's folders its files were found in; empty when Cratarr can't see them. */
  folders: string[];
  /** Its folders as the server describes them, to say what to mount. */
  paths: string[];
  /** Cratarr sees its files but cannot write there. */
  read_only: boolean;
};

export type SourceConfig = { id: string; kind: string; enabled: boolean; libraries: Roles; values: Values };

export type SourceView = SourceConfig & {
  label: string;
  /** Secrets are never sent; these are the ones that are set. */
  secrets_set: string[];
  status: SourceStatus;
  choices: Record<string, Choice[]>;
  server_libraries: ServerLibrary[];
  error: string | null;
  /** Each used library's settings as they apply (its own, else its use's and the palette's). */
  policies?: Record<string, { format: 'cd' | 'as_is'; colour: string | null }>;
};

export type Kind = { id: string; label: string; fields: Field[] };

export type Settings = {
  sources: SourceView[];
  kinds: Kind[];
  /** Whether the sources come from the settings file or (first run) the container's environment. */
  origin: 'file' | 'env';
  fixed: { bind: string };
  /** Null when the server has no data folder: settings can be looked at, not saved. */
  settings_file: string | null;
  playlists: number;
  version: string;
  /** Epoch seconds. */
  started: number;
  /** Where the Music libraries' files are, as Cratarr sees them: the playlists live there. */
  music_folder: string | null;
  /** Null when no Lidarr is set. */
  lidarr: LidarrView | null;
  tag_rules: TagRules;
  /** How long albums deleted from Incoming stay in the trash. */
  trash_days: number;
  catalogue: CatalogueStatus | null;
  /** The saved naming template of Music; null: the one the library already follows. */
  naming: string | null;
  /** The libraries' colours to choose from (decision 0034). */
  palette: string[];
};

/** A naming template measured against Music (decision 0013). */
export type NamingView = {
  template: string;
  folders: { matched: number; total: number };
  files: { matched: number; total: number };
  /** An album of the library named by it: folder/first file. */
  example: string | null;
};
/** Measure a template as typed; without one, the template in use. */
export const checkNaming = (template?: string) =>
  quietly<NamingView>(`/api/settings/naming${template === undefined ? '' : `?template=${encodeURIComponent(template)}`}`);

/** How tag values are split when read; the files are never changed. */
export type TagRules = {
  artist_separators: string[];
  genre_separators: string[];
  /** Names containing a separator that are one artist (AC/DC). */
  artist_keep: string[];
  /** Genres containing a separator that are one genre ("Funk / Soul"). */
  genre_keep: string[];
  /** Names given otherwise (0.9.7): one `from` an alias ("Hip-Hop" → "Hip Hop"), several a merge; in what the library
   *  shows, in harvested words, and as a fix written into the files. */
  genre_aliases?: TagAlias[];
  artist_aliases?: TagAlias[];
};
export type TagAlias = { from: string[]; to: string };

export type CatalogueStatus = {
  folders: { path: string; files: number }[];
  files: number;
  tags_pending: number;
  tags_failed: number;
  /** Files with a known id, per server id. */
  linked: Record<string, number>;
  /** Files with a problem, per kind (see FileProblem). */
  issues: Partial<Record<FileProblem['kind'], number>>;
  progress: { doing: string | null; last_pass: number | null; problems: string[] };
};

export const getSettings = () => request<Settings>('/api/settings');

export const getCatalogue = () => quietly<CatalogueStatus>('/api/catalogue');

/** Folders, chosen and shown (decision 0044): a folder in the browser, with how much audio is in it (`more`: past the
 *  count) and whether Cratarr can write there. */
export type FolderEntry = { name: string; path: string; audio: number; more: boolean; writable: boolean };
/** What the browser shows at a place (`path` null: the top, Cratarr's mounts); `parent` null: up is the top. */
export type FolderListing = { path: string | null; parent: string | null; folders: FolderEntry[] };
export const browseFolders = (path?: string | null) =>
  request<FolderListing>(`/api/folders${path ? `?path=${encodeURIComponent(path)}` : ''}`);
/** A chosen folder's row: whether it is there and writable, its disk's free bytes, and the tracks Cratarr knows in it. */
export type FolderStatus = { exists: boolean; writable: boolean; free: number | null; tracks: number | null };
export const folderStatus = (path: string) => quietly<FolderStatus>(`/api/folders/status?path=${encodeURIComponent(path)}`);
/** Music folders Cratarr found that are not chosen yet. */
export const suggestedFolders = () => quietly<{ folders: string[] }>('/api/folders/suggested');

/** A file the catalogue could not read properly. */
export type FileProblem = {
  folder: string;
  rel: string;
  /** broken: cannot be read · damaged_tag: one field unreadable, rest fine · no_tags · no_length */
  kind: 'broken' | 'damaged_tag' | 'no_tags' | 'no_length';
  detail: string | null;
};

export const getFileProblems = () => request<FileProblem[]>('/api/catalogue/problems');

/** The Subsonic accounts (decision 0030): names only, passwords are never sent back. */
export type SubsonicAccounts = { accounts: string[] };
export const getSubsonicAccounts = () => quietly<SubsonicAccounts>('/api/subsonic/accounts');
/** The first account; refused once one exists. */
export const createSubsonicAccount = (username: string, password: string) =>
  post<SubsonicAccounts>('/api/subsonic/accounts', { username, password });
export const changeSubsonicPassword = (username: string, current: string, password: string) =>
  post<SubsonicAccounts>('/api/subsonic/password', { username, current, password });
/** A device signed in on Cratarr's sign-in page (decision 0039); `current`: the one this page runs on. */
export type SignedIn = { id: string; user: string; device: string; created: number; last_used: number; current: boolean };
export const getSignedIn = () => quietly<{ sessions: SignedIn[] }>('/api/auth/sessions');
/** Signs that device out: its next request goes to the sign-in page. */
export const signOutDevice = (id: string) => request<void>(`/api/auth/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });

/** Sign-in everywhere (decision 0043): whether every request needs a sign-in, and switching it (the password again). */
export const getSignInRequired = () => quietly<{ required: boolean }>('/api/auth/required');
export const setSignInRequired = (on: boolean, password: string) =>
  request<{ required: boolean }>('/api/auth/required', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ on, password }) });
/** An API key (decision 0043), for what cannot sign in on a page (Home Assistant): never the key itself. Since
 *  decision 0051 it acts for its owner, and only reads (`read`) or also plays and changes (`control`); kept with what
 *  uses it (`purpose`, the owner, 2026-10-24: under Connections, not Authentication). */
export type KeyScope = 'read' | 'control';
export type ApiKey = { id: string; name: string; created: number; last_used: number | null; owner?: string; scope?: KeyScope; purpose?: 'home_assistant' | null };
/** A new key: `key` is shown in this answer only, never again. */
export const newApiKey = (name: string, password: string, owner?: string, scope?: KeyScope, purpose?: 'home_assistant') =>
  post<{ key: string; id: string; name: string }>('/api/auth/keys', { name, password, owner, scope, purpose });
/** The weather Home Assistant last told (its weather entity's `condition`, °C, `at` epoch s). */
export type Weather = { condition?: string | null; cloud?: number | null; temperature?: number | null; at: number };
/** Connections' Home Assistant: its keys (their last use is when it last called) and the weather it last told. */
export const getHomeAssistant = () => quietly<{ keys: ApiKey[]; weather: Weather | null; on: boolean }>('/api/connections/home-assistant');
/** Home Assistant switched on or off: off, its keys are kept but refused. */
export const setHomeAssistantOn = (on: boolean) => putJson('/api/connections/home-assistant', { on });

/** What a user may do beyond listening (decision 0051), in Cratarr and in OpenSubsonic apps alike: keep tracks to play
 *  offline, make playlists, share them with the other users, make public share links. */
export type Permissions = {
  offline: boolean;
  playlists: boolean;
  share: boolean;
  links: boolean;
  /** Requests (decision 0070): ask for albums, have that approved at once, answer everyone's. */
  request: boolean;
  auto_approve: boolean;
  manage_requests: boolean;
};
/** A device that belongs to an account (it plays only on its own, decision 0051): `id` is its player's name. One that is
 *  connected, or a Kodi (removed under Players), is not taken away. */
export type OwnDevice = { id: string; name: string; kind: string; online?: boolean; last_seen: number | null };
/** Whether a device can be taken away now. */
export const removable = (d: OwnDevice) => !d.online && d.kind !== 'kodi';
/** One of an account's sign-ins; `current`: the one this page runs on (one's own only). */
export type UserSession = { id: string; device: string; created: number; last_used: number; current?: boolean };
/** Whoever is signed in (decision 0051): their name (from the directory), whether they administer this server, what they
 *  may do, whether they have set a password for OpenSubsonic apps, their devices and sign-ins. */
export type Me = {
  name: string | null;
  /** The directory's display name (LDAP's displayName, 0.10); none for a local account or a server before it. */
  display_name?: string | null;
  admin: boolean;
  /** Deactivated: left the directory's music group (their data kept). */
  active?: boolean;
  /** Signs in through the directory (else Cratarr's own account). */
  directory?: boolean;
  permissions: Permissions;
  app_password?: boolean;
  devices?: OwnDevice[];
  sessions?: UserSession[];
};
export const getMe = () => quietly<Me>('/api/me');
/** What this server can do (0.11, lib/capabilities): the names of what it has configured and what its code supports. */
export const getCapabilities = () => quietly<import('./generated/Capabilities').Capabilities>('/api/capabilities');
/** Ends one of one's own sign-ins, or takes one of one's own devices away. */
export const endMySession = (id: string) => request<void>(`/api/me/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const removeMyDevice = (id: string) => request<void>(`/api/me/devices/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** The server's version and when it started, for everyone (the rest of its settings are its administrators'). */
/** About Cratarr: its version, since when it runs, its licence (SPDX) and where its source is. */
export const getAbout = () => quietly<{ version: string; started: number; licence?: string; source?: string }>('/api/about');
/** Cratarr restarted (0.9.6; its administrators'): it answers, then stops as `docker stop` stops it, and Docker's
 *  restart policy starts it again. */
export const restartServer = () => post<{ restarting: boolean }>('/api/server/restart', {});
/** Whether the server answers at all (its health), as a restart waits for it to be back. */
export async function serverAnswers(): Promise<boolean> {
  try {
    return (await fetch('/api/health', { cache: 'no-store' })).ok;
  } catch {
    return false;
  }
}
/** One's own settings (decision 0051), one object per account, as the pages and the app name its parts; `version`
 *  counts its saves (0: none kept yet). A save names the version it changed, and is refused (409) if another came first. */
export type MySettings = { version: number; settings: Record<string, unknown> };
export const getMySettings = () => quietly<MySettings>('/api/me/settings');
export const putMySettings = (base: number, settings: Record<string, unknown>) =>
  request<{ version: number }>('/api/me/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base, settings }) });
/** Each server's own name and libraries, and what each library is used as, for everyone (the filter): the part of the
 *  server's settings that is not its administrators' alone. */
export type LibrariesView = {
  sources: { id: string; status: { server_name: string | null }; server_libraries: { id: string; name: string }[]; libraries: { music: string[]; incoming: string[] } }[];
};
export const getLibraries = () => quietly<LibrariesView>('/api/libraries');
/** Their own password for OpenSubsonic apps, whose sign-in needs the password itself (a directory never gives it). */
export const setAppPassword = (password: string) =>
  request<void>('/api/me/subsonic-password', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password }) });
/** Every user, for the administrators' Users panel: their profile, permissions, devices, sign-ins. `directory`: an
 *  account from the directory (its name and role set there); else Cratarr's own (decision 0051, amended 2026-10-19). */
export type User = { name: string; admin: boolean; active: boolean; directory?: boolean; permissions: Permissions; devices: OwnDevice[]; sessions: UserSession[] };
export const getUsers = () => request<{ users: User[] }>('/api/users').then((r) => r.users);
const putJson = (url: string, body: unknown) => request<void>(url, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
const userUrl = (name: string, what: string) => `/api/users/${encodeURIComponent(name)}/${what}`;
/** A Cratarr account of its own (the first one there is administers, whatever is asked). */
export const createUser = (name: string, password: string, admin: boolean) => post<User>('/api/users', { name, password, admin });
/** A Cratarr account's role (never one's own, never the last active administrator's). */
export const setUserRole = (name: string, admin: boolean) => putJson(userUrl(name, 'role'), { admin });
/** A Cratarr account's new password, set by an administrator; its other sign-ins end. */
export const setUserPassword = (name: string, password: string) => putJson(userUrl(name, 'password'), { password });
/** A Cratarr account switched off (its data kept, its sign-ins ended) or on again. */
export const setUserActive = (name: string, active: boolean) => putJson(userUrl(name, 'active'), { active });
/** One's own password, for a Cratarr account (it is its OpenSubsonic one too). */
export const changeOwnPassword = (current: string, password: string) => putJson('/api/me/password', { current, password });
export const setPermissions = (name: string, p: Permissions) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/permissions`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(p) });
/** Ends a user's sign-in (an administrator, or the user themselves). */
export const endSession = (name: string, id: string) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
/** Takes a device from a user: it plays for nobody until it signs in again. */
export const removeDevice = (name: string, id: string) =>
  request<void>(`/api/users/${encodeURIComponent(name)}/devices/${encodeURIComponent(id)}`, { method: 'DELETE' });
export const revokeApiKey = (id: string) => request<void>(`/api/auth/keys/${encodeURIComponent(id)}`, { method: 'DELETE' });

/** A directory (LDAP) asked first at sign-in (decision 0040): set by the operator in `ldap.json`, shown read-only. */
export type DirectoryConfig = {
  enabled: boolean;
  url: string;
  starttls: boolean;
  base_dn: string;
  user_attribute: string;
  group: string;
  bind_dn: string;
  bind_password_file: string;
  accounts: Record<string, string>;
};
export type DirectorySettings = { config: DirectoryConfig; bind_password: 'set' | 'not set' | 'unreadable' };
export const getDirectory = () => quietly<DirectorySettings>('/api/auth/ldap');

/** Importing an account's plays, favourites, ratings and added dates from a server into a Subsonic account. */
export type ImportWhat = { plays: boolean; favorites: boolean; ratings: boolean; added: boolean };
/** A server that can import: its accounts (`id` is the server's), and the choices made last time. */
export type ImportSource = {
  /** A connected server's id, or `subsonic:<address>` for a Subsonic server imported from (decision 0067). */
  id: string;
  users: { id: string; name: string }[];
  /** A Subsonic server imported from: its password is asked for each run. */
  password?: boolean;
  user?: string | null;
  account?: string | null;
  what?: ImportWhat;
  error?: string;
};
export type ImportSources = { accounts: string[]; sources: ImportSource[] };
export type ImportRequest = { source: string; user: string; account: string; what: ImportWhat; preview: boolean; password?: string };
export type ImportCounts = { plays: number; songs: number; starred: number; unstarred: number; rated: number; added: number; skipped: number };
export type ImportJob = ImportRequest & {
  started: number;
  finished: number | null;
  step: { name: 'tracks' | 'groups' | 'added'; read: number; total: number | null } | null;
  counts: ImportCounts;
  error: string | null;
  note: string | null;
};
export const getImportSources = () => quietly<ImportSources>('/api/subsonic/import/sources');
/** The import running now, or the last one. */
export const getImport = () => quietly<{ job: ImportJob | null }>('/api/subsonic/import');
export const startImport = (r: ImportRequest) => post<{ job: ImportJob }>('/api/subsonic/import', r);
/** An import from a server undone as a whole (decision 0067): what it took back. */
export type ImportUndone = { items: number; plays: number; unstarred: number; unrated: number };
export const undoImport = (source: string, user: string, account: string) =>
  request<{ undone: ImportUndone }>('/api/subsonic/import', { method: 'DELETE', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ source, user, account }) });

/** Lidarr on the Settings page; the key is never sent. */
export type LidarrView = {
  url: string;
  key_set: boolean;
  enabled: boolean;
  /** What Discover's Want adds an album with; null: as the artists Lidarr has. */
  quality_profile: number | null;
  metadata_profile: number | null;
  root_folder: string | null;
  version: string | null;
  error: string | null;
};

/** What the page sends for Lidarr: a blank key keeps the saved one while the address stays; a blank address removes it. */
export type LidarrDraft = {
  url: string;
  api_key: string;
  enabled?: boolean;
  quality_profile?: number | null;
  metadata_profile?: number | null;
  root_folder?: string | null;
};
/** The saved Lidarr's quality and metadata profiles ([id, name]) and root folders: Want's choices on its page. */
export const getLidarrOptions = () => quietly<{ quality: [number, string][]; metadata: [number, string][]; roots: string[] }>('/api/settings/lidarr/options');
/** The server's own Last.fm key for Discover (artists like each account's own most played, decision 0069 amended): in
 *  use, and saved here (else the container's LASTFM_API_KEY gives it). The key itself is never sent. */
/** `set`: in use (false while switched off); `saved`: a key kept; `enabled`: switched on (0.9.6). */
export const getLastfmServerKey = () => quietly<{ set: boolean; saved: boolean; enabled?: boolean }>('/api/settings/lastfm');
/** Switched on or off, the key kept (0.9.6: off, no Discover artists from it). */
export const setLastfmEnabled = (enabled: boolean) => putJson('/api/settings/lastfm', { enabled });
/** The server's own Last.fm key saved at once; an empty one takes it away. */
export const setLastfmServerKey = (key: string) => putJson('/api/settings/lastfm', { key });
/** The server's Discogs app (0.9.5): set or not, and the requests a minute Discogs last allowed. The secret is never
 *  sent back; both empty take it away. */
export type DiscogsSetting = { set: boolean; per_minute: number | null; enabled?: boolean };
export const getDiscogs = () => quietly<DiscogsSetting>('/api/settings/discogs');
export const setDiscogs = (key: string, secret: string) =>
  request<DiscogsSetting>('/api/settings/discogs', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key, secret }) });
/** Switched on or off, the keys kept (0.9.6: off, Discogs is asked nothing). */
export const setDiscogsEnabled = (enabled: boolean) =>
  request<DiscogsSetting>('/api/settings/discogs', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled }) });
/** The server's slskd, Soulseek (0.9.8: single-track replacement): its address, its API key kept or not (`set`; the
 *  key is never sent back), the folder its completed downloads are in as Cratarr sees them, and what it answered: its
 *  version, or what is wrong. */
export type SlskdSetting = { url: string; set: boolean; folder: string; enabled: boolean; version?: string; error?: string };
export const getSlskd = () => quietly<SlskdSetting>('/api/settings/slskd');
/** Saved at once; without a key the kept one stays, an empty address takes it away. */
export const setSlskd = (s: { url: string; key?: string; folder: string }) =>
  request<SlskdSetting>('/api/settings/slskd', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(s) });
/** Switched on or off, the key kept (0.9.8: off, slskd is asked nothing). */
export const setSlskdEnabled = (enabled: boolean) =>
  request<SlskdSetting>('/api/settings/slskd', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled }) });
/** The server's AcoustID key (0.9.8: single-track replacement fingerprints what it fetched): set or not, never sent
 *  back; an empty one takes it away. */
export type AcoustidSetting = { set: boolean };
export const getAcoustid = () => quietly<AcoustidSetting>('/api/settings/acoustid');
export const setAcoustid = (key: string) =>
  request<AcoustidSetting>('/api/settings/acoustid', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key }) });
/** How Cratarr's MusicBrainz questions stand (0.9.6; administrators'): when it last answered or failed (since the
 *  server started), its pace, each background pass (how many of the albums with a release it has asked about). */
export type MusicBrainzStatus = {
  enabled: boolean;
  last_ok: number | null;
  last_error: { at: number; message: string } | null;
  pace_secs: number;
  passes: { name: string; done: number; total: number; running: boolean; next_at: number | null }[];
  asked: number;
  of: number;
};
export const getMusicBrainz = () => quietly<MusicBrainzStatus>('/api/settings/musicbrainz');
/** Switched on or off (off: no background passes, and Find more music, Renumber and artist lookups say so). */
export const setMusicBrainzEnabled = (enabled: boolean) =>
  request<MusicBrainzStatus>('/api/settings/musicbrainz', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled }) });

/** Check a Lidarr as filled in; nothing is saved. Null for a blank address. */
export const previewLidarr = (draft: LidarrDraft) => post<LidarrView | null>('/api/settings/lidarr', draft);

/** `lidarr` left out: keep the saved one. */
export const saveSettings = (
  sources: SourceConfig[],
  extra: { tag_rules: TagRules; lidarr?: LidarrDraft; trash_days: number; naming?: string },
) =>
  request<Settings>('/api/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sources, ...extra }),
  });

/** One source as it is being filled in: status, choices and libraries, nothing saved. */
export const previewSource = (source: SourceConfig) => post<SourceView>('/api/settings/preview', { source });

export type AlbumTrack = {
  file: string;
  key: string | null;
  bytes: number;
  seconds: number | null;
  kbps: number | null;
  plays: number;
  favourite: boolean;
};
export type Album = {
  folder: string;
  tracks: AlbumTrack[];
  total_bytes: number;
  cover_bytes: number | null;
  plays: number;
  favourites: number;
  playlist_lines: [string, number][];
};
export type Comparison = {
  same_track_count: boolean;
  matched: number;
  same_length: number;
  same_size: number;
  verdict: 'same' | 'similar' | 'different';
};
export type AlbumGroup = {
  albums: Album[];
  comparisons: Comparison[];
  suggestion: { keep: string; reasons: string[] };
};

export const getAlbumDuplicates = () => request<{ groups: AlbumGroup[] }>('/api/albums/duplicates');
/** What a health check finds (decision 0063): its domain, a short title, how many (of how many, for a share), how much it
 *  asks of the owner, when it was first found (epoch s), and where it is dealt with. */
export type HealthFinding = {
  domain: string;
  key: string;
  /** Its short name, the panel's breadcrumb ("Quality replacements"; 0.9.6); a server before it has none. */
  name?: string;
  /** What it is, in a sentence ("11 better copies wait to replace Music"): its card's, and the panel's top. */
  title: string;
  detail?: string;
  count: number;
  of?: number;
  level: 'critical' | 'warning' | 'info';
  since: number;
  /** Where it is dealt with; `thing`: a connection's own page in Settings (0.9.6: lastfm, discogs, musicbrainz). */
  open?: { page: string; switch?: string; thing?: string };
  /** What it is about, one by one: an album (server, id) or a playlist opens where it is. */
  items?: HealthItem[];
};
/** One thing a finding is about: an album (its server and id), a playlist, the files it names (at most 30). */
export type HealthItem = {
  title: string;
  line?: string;
  album?: [string, string];
  playlist?: string;
  files?: string[];
  /** An album missing tracks (0.9.7): those it lacks ("06 · Title", "Disc 2 · 03 · Title"), its release group to ask
   *  for the whole album by, and its request where one is out. */
  missing?: string[];
  release_group?: string;
  request?: RequestMark | null;
  /** A track's own (2026-10-28: a file cut off or breaking off): its song id in `album`, for a better copy of it. */
  track?: string;
};
/** Every domain's findings, worst first, the most recent first among equals; when each domain was checked. */
export const getHealthReport = () => request<{ findings: HealthFinding[]; checked: Record<string, number> }>('/api/health/report');

/** An album in a server's library list (Incoming, Collection). */
export type ListedAlbum = {
  /** Not the library's (a Bandcamp album): asked for, where it stands (lib/CoverStatus). */
  request?: RequestMark | null;
  id: string;
  title: string;
  artist: string | null;
  year: number | null;
  /** When the source added it, epoch seconds. */
  added: number | null;
  path: string | null;
  tracks: number;
  /** e.g. "FLAC", "MP3"; null when mixed or unknown. */
  format: string | null;
  /** The lowest track bitrate. */
  bitrate_kbps: number | null;
  /** The source's own tags, as they are. */
  tags: string[];
  /** The library it was listed from (the filter's library switches). */
  library?: string;
  /** Its artists one by one, when the source credits several; absent: `artist` alone. */
  artists?: string[];
  /** Its length (s), its tracks' together; null where one of them is not known (0.10; a server before it gives none). */
  duration_secs?: number | null;
  /** Classical (0.10): the server's word (a classical genre and one confirmation, or a mark), a kind beside its release
   *  type; `classical_mark` where someone marked it; its works' composers (`key`: a MusicBrainz artist, or `name:…`); its
   *  performers, conductor, orchestra and soloists first. A server before it gives none. */
  classical?: boolean;
  classical_mark?: 'classical' | 'not';
  composers?: { key: string; name: string }[];
  performers?: string[];
};

/** One server's album list. */
export type ShelfSource = {
  id: string;
  label: string;
  /** False when this kind of source cannot list albums at all. */
  supported: boolean;
  albums: ListedAlbum[];
  error: string | null;
  /** When the server read this list from the source, epoch seconds. */
  as_of: number;
  /** The libraries it was read from (the filter's library switches). */
  libraries?: string[];
  /** Those libraries' names, uses, colours and formats (the Library's list, decision 0034). */
  library_info?: LibraryInfo[];
};

/** A used library of a server (decision 0034): the dot on its albums (none for the primary, Music), and what a move
 *  into it does to the files. */
export type LibraryInfo = { id: string; name: string; use: 'music' | 'incoming'; colour: string | null; format: 'cd' | 'as_is' };

/** An album of a library other than Music, as it stands to Music: an upgrade of an album there, or a duplicate. */
export type Verdict = { kind: 'upgrade' | 'duplicate'; library: string; why: string };

/** A server's albums of every used library in one list (decision 0034), with what holds between them. */
export type AlbumsSource = ShelfSource & {
  library_info: LibraryInfo[];
  verdicts: Record<string, Verdict>;
  /** Per album id of an Incoming library: what it lacks of what the library requires (cover, year, genre, …). */
  lacks?: Record<string, string[]>;
  /** Per Music album: an album of another library that upgrades or duplicates it. */
  waiting: Record<string, { kind: 'upgrade' | 'duplicate'; album: string }>;
  deletable: string[];
};
export type AlbumsView = { sources: AlbumsSource[]; trash_days: number; music: string | null };

/** Every album of each server's used libraries (the Library page). `fresh` asks past the service worker's copy (which
 *  answers at once, see sw.js) for the server's latest. */
export const getAlbums = (source?: string, fresh = false) => quietly<AlbumsView>(`/api/albums${follow(source)}`, fresh ? { cache: 'no-cache' } : undefined);

// Classical (0.10): its composers, each one's records, a record's mark, and a track's other versions (the library's
// recordings of the same work, by MusicBrainz's work links).
export const getClassicalComposers = () => quietly<import('./recordKindRules').Composer[]>('/api/classical/composers');
export const getClassicalComposer = (key: string) => quietly<{ key: string; name: string; records: string[] }>(`/api/classical/composers/${encodeURIComponent(key)}`);
/** Marks a record classical or not, library-wide (administrators'); null: as Cratarr finds it. */
export const markClassical = (source: string, id: string, mark: 'classical' | 'not' | null) =>
  request<{ classical: boolean; classical_mark?: 'classical' | 'not' | null }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(id)}/classical`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ mark }),
  });
export type TrackVersion = { source: string; id: string; title: string; artist: string | null; album: string | null; album_id: string | null; year: number | null; performers?: string[] };
export type TrackVersions = { work: { mbid: string; title: string }; composers: { key: string; name: string }[]; versions: TrackVersion[] };
/** A track's work and the library's other recordings of it; 404 where it has no work link. */
export const getTrackVersions = (source: string, id: string) => quietly<TrackVersions>(`/api/tracks/${encodeURIComponent(source)}/${encodeURIComponent(id)}/versions`);
/** Play an album of a connected server on the followed player; answers with the player's label. */
export const playAlbum = (source: string, album: string) =>
  request<{ player: string }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/play`, { method: 'POST' });
/** Make an album a favourite or not (the album panel's heart). */
export const setAlbumFavorite = (source: string, album: string, on: boolean) =>
  later<{ favorite: boolean }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/favorite?on=${on}`, { method: 'POST' }, () => ({ favorite: on }));
/** The album's cover as the server has it (no picture chosen on this device, no reading again). */
export const serverCoverUrl = (source: string, album: string) =>
  source === 'device'
    ? `/__device/cover/${encodeURIComponent(album)}`
    : // A Bandcamp album's: its page's, relayed (an album's id there is its track 0's, decision 0071).
      source === 'bandcamp'
      ? `/api/cover/bandcamp/${encodeURIComponent(album)}`
      : `/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/cover`;
/** The album's cover as this device shows it: a picture chosen here, else the server's, its address carrying the
 *  cover's stamp (lib/coverStamps: a new cover, a new address, everywhere) and, just after a change made here, a round
 *  of its own (until the stamp has come). Cratarr's own library has no covers of a device's own (the owner,
 *  2026-10-25: its administrators upload one for everyone instead), so one chosen here before is passed over. */
export const albumCoverUrl = (source: string, album: string) => {
  const key = coverKey(source, album);
  const v = [coverStamps[key], coverRounds[key]].filter(Boolean).join('-');
  return (source === 'cratarr' ? undefined : localCovers[key]) ?? `${serverCoverUrl(source, album)}${v ? `?v=${v}` : ''}`;
};
/** How the owner used an album (decided 2026-10-16: the Library's orders and filters): its plays, when last played
 *  (epoch seconds), its rating (0: none). */
export type AlbumUse = { plays: number; played?: number; rating: number };
/** Every album of a server played or rated, by id; read fresh each time (plays change). */
export const getAlbumUses = (source: string) => quietly<Record<string, AlbumUse>>(`/api/album-uses/${encodeURIComponent(source)}`);
/** An album's rating, 0 to 5; null when its server keeps none. */
export const getAlbumRating = (source: string, album: string) =>
  quietly<{ rating: number | null }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/rating`);
export const setAlbumRating = (source: string, album: string, stars: number) =>
  later<{ rating: number }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/rating?stars=${stars}`, { method: 'POST' }, () => ({ rating: stars }));

export type Transport = 'previous' | 'play-pause' | 'next' | 'stop';

// The local player (decision 0022): a playlist's track ids, a track's details, and handing a queue to the server's
// player (from a track and a position).
export const playlistTrackIds = (id: string, source?: string) =>
  request<{ source: string; ids: string[]; unknown: number }>(`/api/playlists/${encodeURIComponent(id)}/tracks${follow(source)}`);
const trackPath = (source: string, id: string) => `/api/track/${encodeURIComponent(source)}/${encodeURIComponent(id)}`;

export const trackDetails = (source: string, id: string) =>
  quietly<{ id: string; name: string; artist: string | null; album: string | null; album_id: string | null }>(trackPath(source, id));

/** A given track's playlists and favourite: for the local player's track (decision 0022), what `State` is for the
 *  server's player's. `favorite` null: not known. */
export type TrackState = { addable: boolean; album_id: string | null; favorite: boolean | null; playlists: Playlist[]; info: TrackInfo | null };

export const trackState = (source: string, id: string) => quietly<TrackState>(`${trackPath(source, id)}/playlists`).then((s) => knownState(source, id, s));
/** Each track's state as last told, for a change kept for later to answer with (lib/later). */
const trackStates = new Map<string, TrackState>();
const knownState = (source: string, id: string, s: TrackState) => (trackStates.set(`${source}/${id}`, s), s);
const stateOf = (source: string, id: string): TrackState =>
  trackStates.get(`${source}/${id}`) ?? { addable: false, album_id: null, favorite: null, playlists: [], info: null };

/** A track's file as it is (the track menu's Details): where it lies (`path` within `folder`), its audio, its size,
 *  when it was changed and came in (epoch s), and every tag in the file's order. */
export type TrackFile = {
  path: string;
  folder: string | null;
  format: string | null;
  bitrate_kbps: number | null;
  sample_rate_hz: number | null;
  bit_depth: number | null;
  channels: number | null;
  duration_ms: number | null;
  /** Its tempo (decision 0035): the BPM tag, else the analysis'; none without a beat. */
  bpm?: number | null;
  size: number | null;
  modified: number | null;
  added: number | null;
  /** Its genres as a genre in the panel matches them. */
  genres: string[];
  tags: { name: string; value: string }[];
};
export const trackFile = (source: string, id: string) => quietly<TrackFile>(`${trackPath(source, id)}/details`);

/** Put the track in the playlist or take it out (the state wanted, as `setMember`). */
export const setTrackMember = (source: string, id: string, playlistId: string, member: boolean) =>
  later<TrackState>(`${trackPath(source, id)}/playlists/${encodeURIComponent(playlistId)}?member=${member}`, { method: 'POST' }, () => {
    const s = stateOf(source, id);
    return { ...s, playlists: s.playlists.map((p) => (p.id === playlistId ? { ...p, member } : p)) };
  }).then((s) => knownState(source, id, s));

export const setTrackFavorite = (source: string, id: string, on: boolean) =>
  later<TrackState>(`${trackPath(source, id)}/favorite?on=${on}`, { method: 'POST' }, () => ({ ...stateOf(source, id), favorite: on })).then((s) =>
    knownState(source, id, s),
  );

/** Which of these tracks (ids on `source`) are favourites, in one request: the hearts on a list's rows (decision 0023).
 *  Tracks the server cannot tell about are left out. */
export const getFavorites = (source: string, ids: string[]) =>
  quietly<Record<string, boolean>>(`/api/favorites/${encodeURIComponent(source)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids }),
  });

/** The favourite albums of a server (their ids): the Library's Favourites only and the albums' hearts. */
export const getFavoriteAlbums = (source: string) => quietly<string[]>(`/api/favorites/${encodeURIComponent(source)}/albums`);
/** A server's favourite artists, by name (the Library's Artists, Favourites only). */
export const getFavoriteArtists = (source: string) => quietly<string[]>(`/api/favorites/${encodeURIComponent(source)}/artists`);
/** Cratarr's own favourite tracks as rows (Home's Favourite tracks), at most 100. */
export const getFavoriteTracks = () => quietly<{ source: string; rows: PlaylistRow[] }>('/api/favorites/cratarr/tracks');

/** Which playlists (by name) are favourites, as Finamp's playlist hearts (decision 0023). */
export const getPlaylistFavorites = (source?: string) => quietly<Record<string, boolean>>(`/api/playlists/favorites${follow(source)}`);

export const setPlaylistFavorite = (id: string, on: boolean, source?: string) => {
  const q = new URLSearchParams({ on: String(on) });
  if (source) q.set('source', source);
  return later<{ playlist: string; favorite: boolean }>(`/api/playlists/${encodeURIComponent(id)}/favorite?${q}`, { method: 'POST' }, () => ({ playlist: id, favorite: on }));
};

/** The local player played most of the track: one play counts in the media server (decision 0022). */
/** A track heard (decision 0064), as a player tells it: `id` its own, so a report sent again is one; `at` epoch ms. */
export type HeardEvent = {
  id: string;
  source: string;
  track: string;
  at: number;
  ended: 'finished' | 'skipped' | 'stopped';
  counts: boolean;
  heard_secs: number;
  length_secs: number | null;
  player: string | null;
  context: string | null;
};
/** Home's looks back (the owner, 2026-10-23): Blast from the past, the favourite tracks unplayed longest; Grave digging,
 *  the albums most like the taste and furthest from the listening (`last_played` null: never). Times epoch seconds. */
export type Digging = {
  past: { id: string; name: string; artist: string; album: string; album_id: string; last_played: number }[];
  graves: { id: string; last_played: number | null }[];
};
/** Of these albums (Cratarr's), those the account heard 5% or more of: not new to it (Home's New for you). */
export const heardAlbums = (ids: string[]) => post<{ heard: string[] }>('/api/home/heard', { ids });
/** Home's sections the server puts together (app::home, the owner 2026-10-24): New for you, Jump back in (the asking
 *  player's queues among it), Sounds like your week. */
export const getHomeNew = () => quietly<{ albums: (ListedAlbum & { source: string })[] }>('/api/home/new');
export type HomeBack =
  | { kind: 'album'; at: number; source: string; id: string; title: string; by: string | null }
  | { kind: 'playlist'; at: number; id: string; title: string }
  | { kind: 'queue'; at: number; id: string; title: string; tracks: number; from: string | null };
export const getHomeBack = (player: string | null) =>
  quietly<{ items: HomeBack[] }>(`/api/home/back${player ? `?${new URLSearchParams({ player })}` : ''}`);
export type HomeWeek = {
  name: string;
  source: string;
  total: number;
  genres: string[];
  played: number;
  covers: { track: string; album_id: string | null }[];
};
export const getHomeWeek = (source: string | undefined, without: string[] | undefined) =>
  quietly<{ week: HomeWeek | null }>(`/api/home/week?${new URLSearchParams({ ...(source ? { source } : {}), without: (without ?? []).join(',') })}`);
/** Home's Artists to explore: artists in a random order, the first open (its TheAudioDB description and photo, the
 *  server's artist store), each with its biggest albums (Cratarr's ids). */
export type ExploredArtist = { id: string; name: string; albums: string[]; bio: string | null; photo: boolean };
export const getHomeArtists = (source?: string, without?: string[]) =>
  quietly<{ artists: ExploredArtist[] }>(`/api/home/artists?${new URLSearchParams({ ...(source ? { source } : {}), ...(without?.length ? { without: without.join(',') } : {}) })}`);
export const artistPhotoUrl = (id: string) => `/api/artists/${encodeURIComponent(id)}/photo`;
/** About an artist (their page): TheAudioDB's description and whether the store has a photo; nulls when unknown. */
/** Tracks of Cratarr's own library matching words, each with its playlist line (Health's missing tracks). */
/** `sure`: the one answer for a missing line (the same title at heart by the same artist, alone at the top). */
export type FoundTrack = { id: string; title: string; artist: string; album: string; album_id: string; line: string; sure?: boolean };
/** The library searched by words (`q`), or found again for a missing playlist line (`line`, lib/HealthMissing). */
export const findTracks = (q: string, line?: string) =>
  request<FoundTrack[]>(`/api/library/find?${new URLSearchParams(line ? { line } : { q })}`);
/** Whose a photo is and under which licence, where it is free to use with credit (Wikimedia Commons', 0.9.3). */
export type PhotoCredit = { author: string; licence: string; url: string };
export const getArtistAbout = (name: string) =>
  quietly<{ id: string | null; bio: string | null; photo: boolean; photo_credit?: PhotoCredit | null }>(`/api/artists/about?name=${encodeURIComponent(name)}`);
export const getDigging = (source?: string, without?: string[]) =>
  quietly<Digging>(`/api/home/digging?${new URLSearchParams({ ...(source ? { source } : {}), ...(without?.length ? { without: without.join(',') } : {}) })}`);
/** An artist's Top recommendations (the owner, 2026-10-23: "a list of tracks which follows taste"): their tracks
 *  that fit the account's taste best, best first (Cratarr's song ids). */
export const artistPicks = (name: string, without?: string[]) =>
  quietly<{ ids: string[] }>(`/api/artists/picks?${new URLSearchParams({ name, ...(without?.length ? { without: without.join(',') } : {}) })}`);
/** Home's moment (the owner, 2026-10-23): named after the weather Home Assistant tells and the time of day ("Music for a Rainy
 *  Evening"), a few albums near it (Cratarr's ids); its radio plays inside the crate `like:now`. */
/** `sky`: the canvas Home paints (sunny, clear, fewclouds, cloudy, foggy, rainy, stormy, snowy, windy, hot, plain); `dark`
 *  after sunset. */
export type Moment = { name: string; sky: string; dark: boolean; albums: string[] };
export const getMoment = (source?: string, without?: string[]) =>
  quietly<Moment>(
    `/api/home/moment?${new URLSearchParams({ ...(source ? { source } : {}), ...(without?.length ? { without: without.join(',') } : {}), tz: String(-new Date().getTimezoneOffset() * 60) })}`,
  );
/** What an import would bring (a preview) or brought (decision 0067): the source's items, those found in the library,
 *  those new; and what was not found, the most first (at most 100 of `missing_tracks`). */
export type ImportTally = { items: number; found: number; new: number; missing: { artist: string; title: string; count: number }[]; missing_tracks: number };
/** Long work on the server (`app::tasks`): where it is (`step`: what it does, how many done, of how many where known),
 *  and once finished its `result` or `error`. Followed on the live channel (`task`), or by `getTasks`. */
export type Task = {
  id: number;
  kind: string;
  preview: boolean;
  started: number;
  finished: number | null;
  step: { name: string; done: number; total: number | null } | null;
  error: string | null;
  result: unknown;
};
type Started = { task: Task };
export const getTasks = () => quietly<{ tasks: Task[] }>('/api/tasks');
/** An album's numbers as a release has them (0.9.6; Health's track numbers): where from, whether its discs stay
 *  (a CD's, a CD-sized digital release's) or run on as one (vinyl, cassette), each track now and after. */
export type Renumbering = {
  from: 'musicbrainz' | 'discogs' | 'files';
  release: { title: string; format: string | null; country: string | null; year: number | null } | null;
  discs: 'kept' | 'flattened';
  why: string;
  tracks: { id: string; file: string; title: string | null; now: { disc: number | null; track: number | null }; to: { disc: number; track: number } }[];
  totals: { discs: number; tracks_per_disc: number[] };
};
export const lookRenumber = (album: string) => post<Started>(`/api/albums/cratarr/${encodeURIComponent(album)}/renumber/look`, {});
/** Writes the numbers shown (the preview's `to`), Undo puts the old ones back. */
export const renumber = (album: string, tracks: { id: string; disc: number; track: number }[], totals?: Renumbering['totals']) =>
  post<{ undo: string[] }>(`/api/albums/cratarr/${encodeURIComponent(album)}/renumber`, { tracks, totals });
/** A fix Cratarr proposes for an album's tags (0.9.7, the administrators' metadata workflow; the owner, 2026-10-26:
 *  Cratarr proposes, an administrator approves, written into the files with Undo): its field, the value now and the one
 *  proposed, where that comes from, and how sure (`certain` ones may be applied as a list in one tap). */
export type Fix = {
  id: string;
  album: string;
  /** `reread`, `length`, `trim`: a track's length (2026-10-28: read again; written into its MP3; set to the sound it
   *  holds), its id `<field>:<song id>`. */
  field: 'year' | 'genre' | 'albumartist' | 'musicbrainz' | 'cover' | 'rule' | 'reread' | 'length' | 'trim';
  from: string | null;
  to: string;
  source: string;
  certainty: 'certain' | 'likely';
  why: string;
  /** A genre fix's every genre and style found, strongest first (0.9.8, Discogs with the rest): several may be
   *  chosen; `to` is the strongest. */
  candidates?: { word: string; kind: 'genre' | 'style'; source: string; certainty: 'certain' | 'likely' }[];
};
export type Fixes = { fixes: Fix[]; certain: number; matching?: boolean };
/** A Health finding's fixes, for every album of it (certain ones first); `matching`: MusicBrainz is still being asked
 *  (ask again). */
export const getFixes = (finding: string) => request<Fixes>(`/api/health/fixes?finding=${encodeURIComponent(finding)}`);
/** An album's fixes wherever it is (Incoming's intake check, or Music). */
export const getIntake = (source: string, album: string) => request<Fixes>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/intake`);
/** Applies fixes (by id, or every certain one of a finding): what was applied, what failed and why, and Undo. */
export const applyFixes = (which: ({ ids: string[] } | { certain: string }) & { chosen?: Record<string, string[]> }) =>
  post<{ undo: string[]; applied: number; failed: { id: string; why: string }[] }>('/api/health/fixes/apply', which);
/** What Discogs says of an album (0.9.8): its labels and catalogue numbers, credits (`tracks`: the positions as Discogs
 *  writes them; none for the whole release), genres and styles, and its page; `{}` where Discogs has none. */
export type AlbumDiscogs = {
  master?: number;
  release?: number;
  year?: number;
  genres?: string[];
  styles?: string[];
  labels?: { name: string; catno?: string }[];
  credits?: { name: string; role: string; tracks?: string }[];
  url?: string;
};
export const getAlbumDiscogs = (source: string, album: string) =>
  quietly<AlbumDiscogs>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/discogs`);
/** What Discogs says of an artist (0.9.8): their profile (plain text), real name, aliases and spellings, members and
 *  the groups they are in, and their page; `{}` where it has none (by the MusicBrainz id when given, else an exact name). */
export type ArtistDiscogs = {
  id?: number;
  name?: string;
  real_name?: string;
  profile?: string;
  aliases?: string[];
  variations?: string[];
  members?: { name: string; active: boolean }[];
  groups?: { name: string; active: boolean }[];
  urls?: string[];
  url?: string;
};
export const getArtistDiscogs = (name: string, mbid?: string | null) =>
  quietly<ArtistDiscogs>(`/api/artists/discogs?name=${encodeURIComponent(name)}${mbid ? `&mbid=${encodeURIComponent(mbid)}` : ''}`);
/** A track as the server describes it (decision 0033, amended: the queue holds identities, Cratarr describes them;
 *  0.9.6): `found` false for one it does not know (its name empty); `favorite` the signed-in account's, null for none. */
export type DescribedTrack = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  duration_secs: number | null;
  addable: boolean;
  favorite: boolean | null;
  found: boolean;
};
/** At most 500 at a time, in the order asked. */
export const describeTracks = (tracks: { source: string; id: string }[]) => post<{ tracks: DescribedTrack[] }>('/api/tracks/describe', { tracks });
/** Stops one of the account's tasks (0.9.6: Change cover's look, once a cover is chosen). */
export const stopTask = (id: number) => request<unknown>(`/api/tasks/${id}`, { method: 'DELETE' });

/** An album Discover suggests (decision 0069): outside the library, with why (one reason a source). */
export type DiscoverAlbum = {
  request?: RequestMark | null;
  key: string;
  album: string;
  artist: string;
  artist_key: string;
  year: number | null;
  release_group: string | null;
  cover: string | null;
  links: { name: string; url: string }[];
  new_artist: boolean;
  why: string[];
  /** Which services suggested it. */
  sources?: string[];
  /** Its album page on Bandcamp: Play previews it there (decision 0071). */
  bandcamp?: string;
  /** How alike to each artist played it was found from (Last.fm's match, 0 to 1); Last.fm's suggestions only. */
  alike?: { seed: string; value: number }[];
  /** Its new artist has a picture, read with the look (only such an artist is featured). */
  artist_photo?: boolean;
  /** How many albums the artist new to the library has released, where the look found out (New to you). */
  artist_albums?: number;
  /** A new artist's own page on Bandcamp (from MusicBrainz, read with the picture), and whether they have anything
   *  there at all (that page, or an album of theirs found there): New to you's order and Listen (2026-10-27). */
  artist_bandcamp?: string | null;
  artist_on_bandcamp?: boolean;
  /** What MusicBrainz calls it (album, EP, single…), its genres, length, track count; Last.fm's listeners (0.9). */
  kind?: string | null;
  genres?: string[];
  length_secs?: number | null;
  track_count?: number | null;
  listeners?: number | null;
  /** Discover's mode it is in (the owner, 2026-10-29): the reason that fits it best, `score` that fit (0 to 1). */
  mode?: 'fresh' | 'alike' | 'gaps' | 'artists';
  score?: number;
  /** When it came out ("YYYY-MM-DD" or "YYYY-MM"), where known: Fresh's order. */
  released?: string;
  /** How often the account played its artist, an artist the library has: Gaps' order. */
  artist_plays?: number;
};
/** An artist the library does not have (Discover's New artists): why, how well they fit, their records that fit best
 *  (keys of `found.albums`, the best first). */
export type DiscoverArtist = { name: string; artist_key: string; photo?: string; why: string[]; score: number; records: string[] };
/** Discover's page: the suggestions as last found (none before the first look), which services it asks, and whether
 *  Want can fetch through Lidarr. */
export type DiscoverView = {
  found: { at: number; albums: DiscoverAlbum[]; artists?: DiscoverArtist[]; notes: string[] } | null;
  services: { listenbrainz: boolean; lastfm: boolean };
  lidarr: boolean;
  /** Each card's request, by its key, if it was asked for (decision 0070). */
  requests?: Record<string, AlbumRequest>;
  /** The account may ask for albums. */
  may_request?: boolean;
  /** The cards (their keys) by one of the account's favourite artists. */
  favourite_artists?: string[];
};
export const getDiscover = () => request<DiscoverView>('/api/discover');
export const refreshDiscover = () => post<Started>('/api/discover/refresh', {});
/** Around artists you play (0.9.7, the server's app::around; the owner, 2026-10-26: a refresh brings other artists,
 *  from all those played): the three artists shown, their related artists' albums (Discover's cards, "Like <artist>"),
 *  and whether the group is still being made (ask again). */
export type AroundView = { seeds: string[]; albums: DiscoverAlbum[]; making: boolean };
export const getAround = () => request<AroundView>('/api/discover/around');
/** Around's Refresh: on to the three most played artists not shown yet (from the top again once all have been). */
export const nextAround = () => post<AroundView>('/api/discover/around/next', {});
/** Want: a request, handed to Lidarr at once where the account may (decision 0070). */
export const wantDiscover = (key: string) => post<{ request: AlbumRequest }>('/api/discover/want', { key });

/** An album's own state beside its request's (decision 0070). */
/** Where an album not in the library that was asked for stands (the owner, 2026-10-25; lib/CoverStatus): wanted, or
 *  downloading (`done`, 0 to 1). The server's, on what it lists (0.9.5); none once it is in the library. */
export type RequestMark = { state: 'wanted' | 'downloading'; done?: number | null };
export type RequestedAlbum =
  | { state: 'requested' | 'looking' | 'in_incoming' | 'in_library' | 'unknown' }
  | { state: 'fetching'; done: number }
  | { state: 'partly_there'; tracks: number; of: number };
/** An album asked for (decision 0070): who asked, its status and who answered, and its album's state. */
export type AlbumRequest = {
  id: number;
  user: string;
  album_key: string;
  artist_key: string;
  release_group: string | null;
  album: string;
  artist: string;
  year: number | null;
  cover: string | null;
  at: number;
  status: 'pending' | 'approved' | 'declined' | 'cancelled';
  reason: string | null;
  decided_by: string | null;
  decided_at: number | null;
  /** What Lidarr said when it was handed it. */
  fetched: string | null;
  /** Absent on a request just made (Want's answer). */
  album_state?: RequestedAlbum;
};
/** The Requests page: the account's own, everyone's waiting ones for those who answer them (else null). */
export type RequestsView = { mine: AlbumRequest[]; waiting: AlbumRequest[] | null; may: Permissions; fetcher: boolean };
/** An album Lidarr is looking for, or fetching (`done`, 0 to 1, and what it says of it). */
export type WatchedAlbum = { album: string; artist: string; release_group: string | null; year: number | null; done: number | null; status: string | null };
export type Watching = { fetching: WatchedAlbum[]; looking: WatchedAlbum[]; more: number };
export const getRequests = () => request<RequestsView>('/api/requests');
/** What Lidarr is looking for and fetching now; null without Lidarr. */
export const getWatching = () => quietly<Watching | null>('/api/requests/watching');
/** Requests answered, one or many: each one's outcome, in words. */
export const decideRequests = (ids: number[], approve: boolean, reason: string | null = null) =>
  post<{ id: number; said: string }[]>('/api/requests/decide', { ids, approve, reason });
export const cancelRequest = (id: number) => post<void>(`/api/requests/${id}/cancel`, {});
/** Not interested: never again, its artist held against it (twice, and they are left out). Its undo token. */
export const notDiscover = (key: string) => post<{ undo: string }>('/api/discover/no', { key });
/** Not interested in an artist (the owner, 2026-10-29): they and their records leave every mode of Discover for good. */
export const notDiscoverArtist = (name: string) => post<{ undo: string }>(`/api/discover/artist/${encodeURIComponent(name)}/not`, {});
/** Hidden: gone from the list, its artist not held against it (unlike Not interested). Its undo token. */
export const hideDiscover = (key: string) => post<{ undo: string }>('/api/discover/hide', { key });
/** A Bandcamp album's tracks to play as a preview (decision 0071), as players hold them; never a play. */
/** An artist new to the library (Discover's New to you): TheAudioDB's description, and whether there is a photo. */
export const getDiscoverArtist = (name: string) =>
  quietly<{ bio: string | null; photo: boolean; photo_credit?: PhotoCredit | null }>(`/api/discover/artist?name=${encodeURIComponent(name)}`);
/** How many studio albums an artist from Discover has made (MusicBrainz: albums, no secondary type), counted when first
 *  asked (0.9.3). */
export const getDiscoverArtistAlbums = (name: string) =>
  quietly<{ albums: number | null }>(`/api/discover/artist/albums?name=${encodeURIComponent(name)}`);
/** An album missing from an artist the library has (their discography as MusicBrainz lists it, 0.9.3). */
export type MissingAlbum = {
  release_group: string;
  title: string;
  artist: string;
  year: number | null;
  kind: string | null;
  cover: string | null;
  /** Its page on Bandcamp, where an open found it (the server keeps a find a month; 0.9.5). */
  bandcamp?: string | null;
  request?: RequestMark | null;
};
/** Every album missing from the library's artists read so far (`artists`: how many), newest first. */
export const getDiscoverMissing = () => quietly<{ albums: MissingAlbum[]; artists: number }>('/api/discover/missing');
/** One artist's missing albums (their page). */
/** `release_group`: one of theirs, so an artist the library lacks is found by it on MusicBrainz (0.9.6); `mbid`
 *  the artist's own. */
export const getArtistMissing = (name: string, hint: { release_group?: string | null; mbid?: string | null } = {}) => {
  const q = new URLSearchParams({ name });
  if (hint.mbid) q.set('mbid', hint.mbid);
  if (hint.release_group) q.set('release_group', hint.release_group);
  return quietly<{ albums: MissingAlbum[]; looking: boolean }>(`/api/artists/missing?${q}`);
};
/** What Discover leaves out for the account: its answers (Not interested: `no`, Hide: `hidden`), the artists left out. */
export type DiscoverExcluded = { key: string; album: string; artist: string; year?: number | null; cover?: string | null; how: 'no' | 'hidden'; at: number };
export const getDiscoverExcluded = () => quietly<{ albums: DiscoverExcluded[]; artists: { artist: string; key: string; at: number }[] }>('/api/discover/excluded');
/** Takes an exclusion back, any time (as Undo): an album's answer, or an artist's every "no". */
export const takeBackExcluded = (what: { key: string } | { artist: string }) =>
  request<void>(`/api/discover/excluded?${new URLSearchParams(what)}`, { method: 'DELETE' });
/** An album's page on Bandcamp, looked for when it is opened (MusicBrainz's link first, the artist's page, the search). */
export const findOnBandcamp = (artist: string, album: string, release_group?: string) =>
  request<{ page: string | null }>(`/api/bandcamp/find?${new URLSearchParams({ artist, album, ...(release_group ? { release_group } : {}) })}`);
/** Want for any album (a missing one): the same request and rights as Discover's Want. */
export const wantAlbum = (a: { release_group?: string; artist: string; album: string; year?: number | null; cover?: string | null }) =>
  post<{ request: AlbumRequest }>('/api/requests/want', a);
/** The words looked up on MusicBrainz (Find more music, 0.9.3): its albums and artists, the surest first, `have` where
 *  the library has them. */
export type MusicBrainzFound = {
  albums: { release_group: string; title: string; artist: string; year: number | null; kind: string | null; cover: string | null; have: boolean; request?: RequestMark | null }[];
  artists: { mbid: string; name: string; disambiguation?: string | null; have: boolean }[];
};
export const musicBrainzSearch = (q: string, limit = 20) =>
  quietly<MusicBrainzFound>(`/api/musicbrainz/search?${new URLSearchParams({ q, limit: String(limit) })}`);
export const discoverArtistPhotoUrl = (name: string) => `/api/discover/artist/photo?name=${encodeURIComponent(name)}`;
/** An artist related to another (the server's app/related.rs): why, their id in the library, or an album of theirs on
 *  Bandcamp. */
export type RelatedArtist = { name: string; mbid?: string; why: string[]; id?: string; bandcamp?: string };
/** An artist's related artists to explore (0.9.2) as last looked up; `looking`: a look runs now. */
export const getRelatedArtists = (name: string) =>
  quietly<{ artists: RelatedArtist[]; looking: boolean }>(`/api/artists/related?name=${encodeURIComponent(name)}`);
export const previewBandcamp = (url: string) =>
  request<{ album: string; artist: string; url: string; tracks: LocalTrack[] }>(`/api/preview/bandcamp?url=${encodeURIComponent(url)}`);
/** Everything an artist has on Bandcamp to hear as previews (New to you's Listen; the owner, 2026-10-27): their own
 *  page's releases, newest first; none where nothing was found. */
export const previewBandcampArtist = (name: string, page?: string | null) =>
  request<{ artist: string; page: string | null; albums: { album: string; url: string; year: number | null; tracks: LocalTrack[] }[] }>(
    `/api/preview/bandcamp/artist?${new URLSearchParams({ name, ...(page ? { page } : {}) })}`,
  );

/** A ListenBrainz user's listens into the asking account's history (decisions 0064 and 0067), as a task. */
/** The ListenBrainz user the account imported from last (kept on the server). */
export const getListenBrainzKept = () => quietly<{ user: string | null }>('/api/history/listenbrainz');
export const importListenBrainz = (user: string) => post<Started>('/api/history/listenbrainz', { user });
/** What importing them would bring, nothing written. */
export const previewListenBrainz = (user: string) => post<Started>('/api/history/listenbrainz/preview', { user });
/** Last.fm into the asking account (decision 0067): its scrobbles into the history, its loved tracks as favourites;
 *  `key` the API key made at Last.fm, else the one kept from last time. */
export type LastFmImported = {
  scrobbles: ImportTally;
  loved: ImportTally;
  /** Every loved track the library does not have, with the album it was most scrobbled from. */
  loved_missing?: { artist: string; title: string; album: string | null }[];
};
export const getLastFm = () => quietly<{ user: string | null; key: boolean }>('/api/history/lastfm');
/** What a Last.fm import brings (0.9.2): its scrobbles, its loved tracks; un-loved ones stop being favourites only
 *  when asked. */
export type LastFmWhat = { scrobbles: boolean; loves: boolean; unstar: boolean };
export const importLastFm = (user: string, key?: string, what?: LastFmWhat) => post<Started>('/api/history/lastfm', { user, key, what });
export const previewLastFm = (user: string, key?: string, what?: LastFmWhat) => post<Started>('/api/history/lastfm/preview', { user, key, what });
export const undoLastFm = (user: string) =>
  request<{ scrobbles: number; unstarred: number }>('/api/history/lastfm', { method: 'DELETE', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ user }) });
/** What a file brought or would bring (decision 0067, step 4): listens into the history (`from` the kind of export), or
 *  a playlist made of the tracks found (its name, and its undo token once made); or Spotify's account data (0073): its
 *  listens, where it has them, and its library's snapshot, with how much of it is here. */
export type FileBrought =
  | { kind: 'history'; from: 'listenbrainz' | 'spotify' | 'lastfm'; tally: ImportTally }
  | { kind: 'playlist'; name: string; tally: ImportTally; undo: string | null }
  | {
      kind: 'spotify';
      history: ImportTally | null;
      library: { albums: number; tracks: number; playlists: number; artists: number; in: number; partly: number; missing: number } | null;
    };
export const importFile = (file: File, preview: boolean) =>
  request<Started>(`/api/import/file?${new URLSearchParams({ name: file.name, preview: String(preview) })}`, { method: 'POST', body: file });
/** The listens imported from files of one kind out of the asking account's history again: how many. */
export const undoFileImport = (from: string) => request<{ removed: number }>(`/api/import/file?${new URLSearchParams({ from })}`, { method: 'DELETE' });
/** A listening service the asking account scrobbles to (decision 0068): its account there, why it stopped (until
 *  connected again), and how many plays wait. Never its secret. */
export type ScrobbleTarget = { target: 'listenbrainz' | 'lastfm'; name: string; broken: string | null; on: boolean; queued: number; waiting: string | null };
export const getScrobbles = () => quietly<{ targets: ScrobbleTarget[] }>('/api/scrobble');
/** ListenBrainz connected with the user token from its settings page (checked there first). */
export const scrobbleListenBrainz = (token: string) => post<{ name: string }>('/api/scrobble/listenbrainz', { token });
/** Last.fm's API key and shared secret kept; the answer is Last.fm's sign-in page, which comes back to `callback`. */
export const scrobbleLastFm = (api_key: string, api_secret: string, callback: string, app: boolean) =>
  post<{ url: string }>('/api/scrobble/lastfm', { api_key, api_secret, callback, app });
/** A listening service switched on or off, its sign-in kept. */
export const setScrobbleOn = (target: string, on: boolean) =>
  putJson(`/api/scrobble/${encodeURIComponent(target)}`, { on });
export const forgetScrobble = (target: string) => request<void>(`/api/scrobble/${encodeURIComponent(target)}`, { method: 'DELETE' });
/** Every listen imported from ListenBrainz out of the asking account's history again: how many. */
export const undoListenBrainz = () => request<{ removed: number }>('/api/history/listenbrainz', { method: 'DELETE' });
/** Tells the server tracks heard; answers with the ids it has (now or before). */
export const reportPlays = (events: HeardEvent[]) => post<{ done: string[] }>('/api/plays', { events });
export const trackPlayed = (source: string, id: string) => quietly<{ counted: boolean }>(`${trackPath(source, id)}/played`, { method: 'POST' });
/** An album as its view in the panel shows it: its tracks in play order, as a playlist's rows (decision 0023). */
/** `id`: the album's, another than asked for when it moved (Keep, Replace) and the server followed it. */
export type AlbumView = {
  source: string;
  id: string;
  title: string;
  artist: string | null;
  year: number | null;
  genres: string[];
  /** "FLAC", "MP3"…; null when mixed or unknown. */
  format: string | null;
  /** Its weakest track's bitrate. */
  bitrate_kbps: number | null;
  rows: PlaylistRow[];
  /** When it came and when it was last played (epoch seconds), and how often (the panel's About); absent where the
   *  server cannot say. */
  added?: number;
  played?: number;
  plays?: number;
  /** What its page says of an album elsewhere (a Bandcamp album's, 0.9.2; the owner, 2026-10-25: "as much as our own
   *  panel"): its release date, label, words about it and credits, its other tags, its pages, how many listen. */
  released?: string;
  label?: string;
  about?: string;
  credits?: string;
  tags?: string[];
  links?: { name: string; url: string }[];
  listeners?: number;
};
/** One search's answer from the server (decision 0072, 0.9.3): per kind, and the kinds merged for Top results. Each
 *  item in lib/search's shape; a track's subject is its album's, the track itself in `track`. */
/** What the server's search finds: the library's kinds, the account's playlists and crates, and what is not in the
 *  library (decision 0072, 7): albums missing from its artists, and what Discover leaves out. */
export type SearchKind = 'album' | 'artist' | 'track' | 'genre' | 'style' | 'mood' | 'tag' | 'label' | 'playlist' | 'crate' | 'missing' | 'excluded' | 'spotify' | 'suggested' | 'requested';
/** An item found: a track's own id in `track`; an artist's library id in `id` (`ar-…`, 0.11: a Sounds like's seed). */
export type ServerSearch = {
  q: string;
  groups: Partial<Record<SearchKind, { total: number; items: (SearchResult & { track?: string; id?: string })[] }>>;
  top: (SearchResult & { track?: string; id?: string })[];
};
/** Searches the server: `kinds` only (all where none), `limit` a kind (20; 100 at most), from `offset` on (a Show all's
 *  next page, decision 0076; a server before 0.9.13 answers from the start again). */
export const search = (q: string, kinds: SearchKind[] = [], limit = 20, offset = 0) =>
  quietly<ServerSearch>(
    `/api/search?${new URLSearchParams({ q, ...(kinds.length ? { kinds: kinds.join(',') } : {}), limit: String(limit), ...(offset ? { offset: String(offset) } : {}) })}`,
  );
/** An album found beyond what Cratarr knows (decision 0076): MusicBrainz's and Discogs' live search, merged; where. */
export type BeyondAlbum = {
  title: string;
  artist: string;
  year: number | null;
  cover: string | null;
  release_group?: string | null;
  discogs?: string | null;
  /** Its page on Discogs, where Discogs found it: Discogs' data is shown with its attribution (its terms). */
  discogs_url?: string | null;
  sources: ('musicbrainz' | 'discogs')[];
  bandcamp?: string | null;
  request?: RequestMark | null;
  have?: boolean;
};
/** Beyond's albums for the words (decision 0076): the server's own merged search where it has it (0.9.13), else
 *  MusicBrainz's alone, as Find more music asks it. */
export type BeyondFound = { albums: BeyondAlbum[]; artists: MusicBrainzFound['artists'] };
export async function searchBeyond(q: string, limit = 20, offset = 0): Promise<BeyondFound> {
  try {
    const r = await request<{ albums: BeyondAlbum[]; artists?: MusicBrainzFound['artists'] }>(
      `/api/search/beyond?${new URLSearchParams({ q, limit: String(limit), ...(offset ? { offset: String(offset) } : {}) })}`,
    );
    return { albums: r.albums, artists: r.artists ?? [] };
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 404)) throw e;
  }
  // A server before 0.9.13: MusicBrainz's alone, one page.
  if (offset) return { albums: [], artists: [] };
  const r = await musicBrainzSearch(q, limit);
  return {
    albums: r.albums.map((a) => ({ title: a.title, artist: a.artist, year: a.year, cover: a.cover, release_group: a.release_group, sources: ['musicbrainz'], request: a.request ?? null, have: a.have })),
    artists: r.artists,
  };
}
/** A wind's plan, without its sound (map=1): where in the track(s) each moment of its stream is. */
export const windMap = (url: string) =>
  quietly<{ map: [number, number, number][]; speed: number; length_ms: number }>(`${url}${url.includes('?') ? '&' : '?'}map=1`);
export const albumView = (source: string, album: string) =>
  quietly<AlbumView>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}`);
/** An artist as their panel shows them (decided 2026-10-16, as Symfonium's), by name as the albums credit them: their
 *  albums and the albums they appear on, newest first, and their tracks album by album with the plays. */
export type ArtistView = {
  source: string;
  name: string;
  /** Their MusicBrainz id, where Cratarr's own library knows it (0.9.8: Discogs found by it, surely). */
  mbid?: string;
  albums: ListedAlbum[];
  appears_on: ListedAlbum[];
  rows: PlaylistRow[];
  /** Null where the server keeps no favourite artists. */
  favorite?: boolean;
};
export const artistView = (source: string, name: string) =>
  quietly<ArtistView>(`/api/artists/${encodeURIComponent(source)}?name=${encodeURIComponent(name)}`);
/** Whether an artist is a favourite (the heart on an album's artist chip); null where the server keeps none. */
export const artistFavorite = (source: string, name: string) =>
  quietly<{ favorite: boolean | null }>(`/api/artists/${encodeURIComponent(source)}/favorite?name=${encodeURIComponent(name)}`);
export const setArtistFavorite = (source: string, name: string, on: boolean) =>
  later<{ favorite: boolean }>(`/api/artists/${encodeURIComponent(source)}/favorite?name=${encodeURIComponent(name)}&on=${on}`, { method: 'POST' }, () => ({ favorite: on }));
/** Where an album of Cratarr's own library was left (Resume): a track and a position, from the bookmarks apps keep
 *  (decision 0030); `track` null when there is none. */
export const albumResume = (album: string) =>
  quietly<{ track: string | null; position_secs?: number }>(`/api/library/albums/${encodeURIComponent(album)}/resume`);
/** The album's resume point gone. */
export const forgetAlbumResume = (album: string) => request<void>(`/api/library/albums/${encodeURIComponent(album)}/resume`, { method: 'DELETE' });
export const handover = (ids: string[], index: number, positionSecs: number, source?: string) =>
  post<{ player: string }>(`/api/player/handover${follow(source)}`, { ids, index, position_secs: positionSecs });
/** Previous / play-pause / next on the player that plays now. */
export const transport = (command: Transport, source?: string) =>
  quietly<{ player: string }>(`/api/player/${command}${follow(source)}`, { method: 'POST' }); // shown at once

/** What a move would do with an album: its new folder in the library, or why not (`needs_year`: ask for the year). */
export type KeepPreview = {
  title: string;
  folder: string | null;
  files: number;
  refused: { kind: 'needs_year' } | { kind: 'other'; detail: string } | null;
  /** Hi-res files converted to 16-bit/44.1 kHz first (the move then waits in the queue). */
  convert: number;
  /** Whether a Lidarr is set to be told to stop fetching it; without one it may fetch the album again. */
  acquisition: boolean;
};
/** What a move or Replace did: done (`undo`), or queued until its hi-res files are converted (`queued`). */
export type AlbumMoved = { undo: string | null; title: string; folder: string; queued: boolean };
const albumPath = (source: string, album: string) => `/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;
/** What moving an album into library `to` would do (decision 0034): its folder there, or why it can't. */
export const movePreview = (source: string, album: string, to: string, year?: string) =>
  request<KeepPreview>(`${albumPath(source, album)}/move?to=${encodeURIComponent(to)}${year ? `&year=${encodeURIComponent(year)}` : ''}`);
/** Move an album into another used library of its server, named and converted that library's way (Undo brings it
 *  back); into a library of CD format with hi-res files it is queued. */
export const moveAlbum = (source: string, album: string, to: string, year?: string) =>
  request<AlbumMoved>(`${albumPath(source, album)}/move`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(year ? { to, year } : { to }),
  });
/** Delete an album of any library, into that library's trash (Undo brings it back). */
export const deleteAlbum = (source: string, album: string) => request<{ undo: string; title: string }>(`${albumPath(source, album)}/delete`, { method: 'POST' });

/** What Replace would do with an upgrade: the album in Music it replaces, how many of its tracks the new files take
 * over, new tracks, and where it goes; or why not (`unmatched`: old tracks without a match, by title). */
export type ReplacePreview = {
  title: string;
  old: string;
  folder: string | null;
  tracks: number;
  added: number;
  refused: { kind: 'needs_year' } | { kind: 'unmatched'; detail: string[] } | { kind: 'other'; detail: string } | null;
  convert: number;
  /** Whether a Lidarr is set to be told to stop fetching it; without one it may fetch the album again. */
  acquisition: boolean;
  /** The album's cover in Music and the new copy's (0.9.5), for Replace's keep-or-new. */
  covers?: { current: string | null; new: string | null };
  /** How the new copy's tags differ from the album's (0.9.7): only the differences, a change the same on every track
   *  once (`track` null). */
  diff?: { track: string | null; field: string; old: string | null; new: string | null }[];
  /** What the album had that the new copy lacks ("Year", "MusicBrainz ids", "3 fewer tags"…). */
  lost?: string[];
  /** What Replace and carry over brings into the new copy ("MusicBrainz ids", "Track numbers", "Cover"…). */
  carry?: string[];
};
const replacePath = (source: string, album: string) =>
  `/api/incoming/replace/${encodeURIComponent(source)}/${encodeURIComponent(album)}`;
export const replacePreview = (source: string, album: string, year?: string) =>
  request<ReplacePreview>(`${replacePath(source, album)}${year ? `?year=${encodeURIComponent(year)}` : ''}`);
/** Replace the album in Music with an upgrade: the old files go to the trash (Undo brings them back). */
export const replaceIncoming = (source: string, album: string, year?: string, cover?: 'keep' | 'new', carry = false) =>
  request<AlbumMoved>(replacePath(source, album), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ...(year ? { year } : {}), ...(cover ? { cover } : {}), ...(carry ? { carry: true } : {}) }),
  });

/** An album in the conversion queue: waiting, converting (`done` of `total` files), or just finished. */
export type Job = {
  id: number;
  source: string;
  album_id: string;
  title: string;
  action: 'keep' | 'replace';
  state: 'waiting' | 'converting' | 'done' | 'failed';
  done: number;
  /** How far the file being converted is, 0 to 1. */
  part: number;
  total: number;
  folder: string;
  undo: string | null;
  why: string | null;
};
export const getJobs = () => quietly<Job[]>('/api/incoming/jobs');
/** Take an album out of the queue while it waits or converts. */
export const cancelJob = (id: number) => request<null>(`/api/incoming/jobs/${id}/cancel`, { method: 'POST' });

/** A name with a count: "Downtempo", 120 albums. */
export type Count = { name: string; n: number };
/** Collection's overview beyond the files: Incoming, growth, and what the account heard (decision 0065). */
export type InsightsOverview = {
  incoming_albums: number;
  /** When the album in Incoming longest came (epoch s). */
  incoming_since: number | null;
  /** Albums added per step of the period, oldest first: days ("2026-10-23"), weeks (their Mondays), months
   *  ("2026-10"), years ("2026"). */
  added: Count[];
  added_step: 'day' | 'week' | 'month' | 'year';
  tracks: number;
  tracks_heard: number;
  albums: number;
  albums_never: number;
  favourites: number;
  /** Tracks by how often played ("0", "1", "2–4", …, "20+"), and the most played. */
  plays: Count[];
  most_played: { title: string; artist: string | null; plays: number }[];
};
/** A genre's or an artist's tracks (`n`) and those of them in a playlist (`listed`). */
export type Layered = { name: string; n: number; listed: number };

/** A top artist, album or track of a period (Insights' Listening, decision 0065). */
export type ListeningTop = { id: string; name: string; by?: string; plays: number; secs: number };
/** The asking account's listening over a week, a month or a year (from the play log, decision 0064). */
export type Listening = {
  period: 'week' | 'month' | 'year' | 'all';
  from: number;
  /** Where the log begins (epoch s); null while it is empty. */
  log_from: number | null;
  total_secs: number;
  plays: number;
  /** Per day (a week, a month) or per week (a year): its start (epoch s) and the seconds heard. */
  time: { start: number; secs: number }[];
  /** Taste over time, further back than the period: per week, month or year (`starts`, oldest first), each top
   *  genre's share of the time listened then (0 to 1); `shares[i][j]`: step i, genre j. */
  taste: { step: 'day' | 'week' | 'month' | 'year'; starts: number[]; genres: string[]; shares: number[][] };
  artists: ListeningTop[];
  albums: ListeningTop[];
  tracks: ListeningTop[];
};
/** Of the libraries switched on (`without`, of the server followed: Insights' switches). */
export const getListening = (period: Listening['period'], source?: string, without?: string[]) => {
  const q = new URLSearchParams({ period, tz: String(-new Date().getTimezoneOffset() * 60) });
  return request<Listening>(`/api/history/listening${crateQuery(q, source, { without })}`);
};

/** The listener's taste and what they avoid, from the one similarity (app/taste.rs): Listening's Profile. */
export type ProfileWord = { name: string; w: number };
/** `lo` and `hi`: the library's own range (its 2nd and 98th percentiles), the axis. */
export type ProfileNumber = { name: string; unit: 'value' | 'share'; mid: number; spread: number; lo: number; hi: number };
/** `lift`: how far above the library's middle, in the library's spreads (what makes a mood stand out). */
export type ProfileMood = { name: string; mid: number; lift: number };
export type ProfileSide = {
  /** Tracks each source added: favourites, playlists, plays (taste); excluded, skipped (avoid). */
  sources: Record<string, number>;
  tracks: number;
  /** Of them, those AudioMuse has heard. */
  heard: number;
  words: { kind: string; title: string; top: ProfileWord[] }[];
  numbers: ProfileNumber[];
  moods: ProfileMood[];
};
export type TasteProfile = { taste: ProfileSide; avoid: ProfileSide };
/** `without` comma-separated, as like-curve takes it; the period narrows plays and skips; `off`: the sources left out. */
export const getTasteProfile = (source?: string, without?: string[], period: Listening['period'] = 'all', off: string[] = []) => {
  const q = new URLSearchParams({ period });
  if (off.length) q.set('off', off.join(','));
  if (source) q.set('source', source);
  if (without?.length) q.set('without', without.join(','));
  return request<TasteProfile>(`/api/history/profile?${q}`);
};

export type Insights = {
  library: {
    tracks: number;
    albums: number;
    artists: number;
    duration_ms: number;
    bytes: number;
    /** Tracks per format. */
    formats: Count[];
    lossless_albums: number;
    below_320_albums: number;
    /** Albums per decade, oldest first ("1970s"). */
    decades: Count[];
    /** Albums per year, oldest first ("1997"). */
    years: Count[];
    /** Tracks per genre / album artist, and those in a playlist; most first. */
    genres: Layered[];
    top_artists: Layered[];
  };
  playlists: {
    playlists: number;
    distinct_tracks: number;
    in_one_playlist: number;
    missing: number;
    library_tracks_in_playlists: number;
    /** Of the same, in one of the account's own crates (not Unlisted). */
    library_tracks_in_crates: number;
    albums_without_playlist: number;
    most_shared: { title: string; artist: string | null; playlists: number }[];
  };
  proximity: Proximity;
  /** Null where Cratarr's own library is not open. */
  overview: InsightsOverview | null;
};

/** Playlists as points and how close each two are, by three signals (0..1 each). */
export type Proximity = {
  /** `focus`: how concentrated its genres are, 0..1 (null without genre tags). */
  /** `shares` (0.9.8): each genre's share of its tracks' genres (the rules' aliases applied, 0.05 and up), for the
   *  Map's genre territories. */
  playlists: { name: string; tracks: number; genres: string[]; focus: number | null; shares?: Record<string, number> }[];
  links: {
    a: number;
    b: number;
    tracks: number;
    genre: number;
    artists: number;
    shared_tracks: number;
    shared_artists: number;
    genres: string[];
  }[];
};

export type Signal = 'tracks' | 'genre' | 'artists';

/** Collection's figures from the libraries switched on (`without`) of the server followed (decision 0065). */
export const getInsights = (source?: string, without?: string[], period: Listening['period'] = 'all') =>
  request<Insights>(`/api/insights${crateQuery(new URLSearchParams({ period }), source, { without })}`);

/** A sync group's visuals (decision 0046, amended): the settings that shape the picture, and the version the server
 *  gave them. None yet: a 404. */
export type GroupVisuals = { version: number; settings: Record<string, unknown> };
export const getGroupVisuals = (group: string) => quietly<GroupVisuals>(`/api/groups/${encodeURIComponent(group)}/visuals`);
/** Settings into a group's visuals (the server merges them key by key, the routes route by route): `base` 0 only where
 *  the group has none yet (else a 409); `writer` names this page, so it knows its own change when it comes back. */
export const putGroupVisuals = (group: string, body: { base: number; settings: Record<string, unknown>; writer: string }) =>
  quietly<{ version: number }>(`/api/groups/${encodeURIComponent(group)}/visuals`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

/** A saved look of the visuals: the settings that shape the picture, named (after the track it was saved on). */
export type VisualsSnapshot = {
  id: string;
  name: string;
  settings: Record<string, unknown>;
  /** Epoch seconds. */
  created: number;
  /** A small picture of the look (a JPEG data URL); none for the first ones saved. */
  picture?: string | null;
  /** The owner's, shown to another account: recalled, not renamed, saved over or deleted. */
  shared?: boolean;
};
export const getVisualsSnapshots = () => quietly<VisualsSnapshot[]>('/api/visuals/snapshots');
export const saveVisualsSnapshot = (name: string, settings: Record<string, unknown>, picture: string | null = null) =>
  request<VisualsSnapshot>('/api/visuals/snapshots', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(picture ? { name, settings, picture } : { name, settings }),
  });
export const renameVisualsSnapshot = (id: string, name: string) =>
  request<VisualsSnapshot>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name }) });
/** A snapshot saved over: the visuals' settings now and a new picture, its name (changed, where given) and time kept. */
export const saveOverVisualsSnapshot = (id: string, settings: Record<string, unknown>, picture: string | null, name?: string) =>
  request<VisualsSnapshot>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ settings, ...(picture ? { picture } : {}), ...(name ? { name } : {}) }),
  });
export const deleteVisualsSnapshot = (id: string) => request<void>(`/api/visuals/snapshots/${encodeURIComponent(id)}`, { method: 'DELETE' });

/** One of Cratarr's own tracks as the Library's Tracks lists them (0.9.4: GET /api/library/tracks). */
export type LibraryTrack = {
  id: string;
  title: string;
  artist: string | null;
  album: string | null;
  album_id: string;
  track_no?: number | null;
  disc_no?: number | null;
  /** Seconds. */
  duration: number;
  year?: number | null;
  /** Epoch seconds. */
  added?: number | null;
  plays: number;
  favorite: boolean;
  format?: string | null;
  bitrate?: number | null;
  /** Its tempo, its album's artist and when the asking account last played it (epoch s; 0.9.7): left out where
   *  unknown. */
  bpm?: number;
  album_artist?: string;
  last_played?: number;
};
export type LibraryTrackOrder = 'title' | 'artist' | 'album' | 'added' | 'year' | 'plays' | 'duration' | 'tempo' | 'albumArtist' | 'lastPlayed';
/** A page of the library's tracks: the words (as the search reads them), the order, the libraries switched off, and
 *  the Library's filter on the tracks themselves (its switches, its conditions, decision 0061). `total`: how many pass,
 *  `all`: how many before the filter. */
export const libraryTracks = (p: {
  q?: string;
  order: LibraryTrackOrder;
  down: boolean;
  offset: number;
  limit: number;
  without?: string[];
  favourites?: boolean;
  unplayed?: boolean;
  topRated?: boolean;
  filter?: Condition[];
}) => {
  const u = new URLSearchParams({ order: p.order, down: String(p.down), offset: String(p.offset), limit: String(p.limit) });
  if (p.q) u.set('q', p.q);
  for (const w of p.without ?? []) u.append('without', w);
  if (p.favourites) u.set('favourites', 'true');
  if (p.unplayed) u.set('unplayed', 'true');
  if (p.topRated) u.set('top_rated', 'true');
  if (p.filter?.length) u.set('filter', JSON.stringify(p.filter));
  return quietly<{ items: LibraryTrack[]; total: number; all?: number; offset: number }>(`/api/library/tracks?${u}`);
};

/** Several albums' tracks in one call (0.9.4): in the albums' order, each album's in its play order; one that can't
 *  be read left out. Counted for the progress bar. */
export const albumsTracks = (albums: { source: string; id: string }[]) =>
  request<{ tracks: { source: string; id: string; name: string; artist?: string | null; album?: string | null; album_id: string; duration_secs?: number | null }[] }>(
    '/api/albums/tracks',
    { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ albums }) },
  );

/** Whether the server has 0.9.4's calls for many at once (its library tracks list answers), asked once: a call it
 *  already had with a new meaning (append's only_new) is used only then. */
let newer: Promise<boolean> | null = null;
export const hasBatches = (): Promise<boolean> => {
  // The server's list says it, where it has one (0.11, lib/capabilities); else asked as before.
  const said = canBatch?.();
  if (said !== null && said !== undefined) return Promise.resolve(said);
  return (newer ??= quietly(`/api/library/tracks?${new URLSearchParams({ offset: '0', limit: '1' })}`).then(
    () => true,
    (e) => !(e instanceof ApiError && (e.status === 404 || e.status === 405)),
  ));
};
/** The capability list's word on batches, given by lib/capabilities (which imports this module, so not the other way). */
let canBatch: (() => boolean | null) | null = null;
export const tellBatches = (f: () => boolean | null) => void (canBatch = f);
/** Those of `ids` a playlist does not hold yet, added in one call (0.9.4). */
export const appendNew = (id: string, source: string, ids: string[]) =>
  post<{ playlist: string; added: number } & Undo>(`/api/playlists/${encodeURIComponent(id)}/append`, { source, ids, only_new: true });
type Batch = { failed: { id: string; error: string }[] };
/** Several playlists at once (0.9.4): favourites, deleted (their undo tokens), shared or not, their rows. */
export const batchFavorite = (ids: string[], on: boolean, source?: string) => post<Batch & { done: string[] }>('/api/batch/playlists/favorite', { ids, on, source });
export const batchDelete = (ids: string[]) => post<Batch & { deleted: string[]; undo: string[] }>('/api/batch/playlists/delete', { ids });
export const batchLink = (ids: string[], on: boolean, target?: string) => post<Batch & { done: string[] }>('/api/batch/playlists/link', { ids, on, target });
export const batchRows = (ids: string[], source?: string) =>
  post<{ playlists: ({ id: string; source: string; rows: PlaylistRow[] } | { id: string; error: string })[] }>('/api/batch/playlists/rows', { ids, source });
/** A call the server does not have (one before 0.9.4): the caller goes one by one instead. */
export const missingCall = (e: unknown) => e instanceof ApiError && (e.status === 404 || e.status === 405);

/** An album's covers to choose from (0.9.5; the owner: "the option to control covers of albums when I'm not happy
 *  with one"): its own, and Discogs' (its master's and releases') and the Cover Art Archive's, each with its size and
 *  what it is ("Master", "CD · Germany · 2012", "Front"). Albums whose files Cratarr holds. */
export type CoverChoice = { url: string; width: number | null; height: number | null; from?: 'discogs' | 'coverartarchive'; what?: string };
export const getAlbumCovers = (source: string, album: string) =>
  request<{ current: CoverChoice | null; candidates: CoverChoice[] }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/covers`);
/** One of them as the album's cover, inside every file and beside them; its undo token. */
/** Change cover's look as a task (0.9.6; kind `covers:<album>`), its steps per source (MusicBrainz, the Cover Art
 *  Archive, Discogs), its result the same as getAlbumCovers'. One already running for the album is given back. */
export const lookAlbumCovers = (source: string, album: string) =>
  post<Started>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/covers/look`, {});
export const setAlbumCover = (source: string, album: string, url: string) =>
  post<{ undo: string[] }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/covers`, { url });
/** A picture of one's own as the album's cover, for everyone (its administrators', the owner 2026-10-25): written as a
 *  chosen cover is, inside the files and beside them; Undo puts the old one back. JPEG, PNG or WebP, kept as sent, at
 *  most COVER_MAX (a FLAC's picture holds 16 MiB, and every track carries it). */
export const COVER_MAX = 12 * 1024 * 1024;
export const uploadAlbumCover = (source: string, album: string, file: Blob) =>
  request<{ undo: string[] }>(`/api/albums/${encodeURIComponent(source)}/${encodeURIComponent(album)}/covers`, {
    method: 'PUT',
    headers: { 'Content-Type': file.type || 'application/octet-stream' },
    body: file,
  });

/** The account's play history deleted, and what was learned from it (0.9.5): how many plays went. */
export const deleteHistory = () => request<{ removed: number }>('/api/me/history', { method: 'DELETE' });

// --- Spotify (decision 0073): the signed-in account's Spotify library, from its export or its own live connection ---
/** Where an item of it stands here: in the library, some of it, none, or asked for (0070). */
export type SpotifyStatus = 'in' | 'partly' | 'missing' | 'requested';
/** What the account's Spotify lists came from: its own app's live connection (`live`, Premium), the export it uploaded
 *  (`imported`); `shown`: which the lists are now (live wins), null while it has none. */
export type SpotifyState = {
  client_id: string | null;
  live: { user: string; name: string; since: number; sync: { state: 'idle' | 'running' | 'failed'; at: number | null; error: string | null } } | null;
  imported: { at: number; file: string } | null;
  shown: 'live' | 'imported' | null;
  counts: { albums: number; tracks: number; playlists: number; artists: number };
};
export type SpotifyAlbum = {
  id: string;
  title: string;
  artist: string;
  year: number | null;
  cover: string | null;
  added_at: number | null;
  status: SpotifyStatus;
  /** Its tracks here, of how many. */
  have: number;
  of: number;
  /** The library's album, where it has it (or some of it). */
  album: { source: string; id: string } | null;
  release_group: string | null;
  request: AlbumRequest | null;
  preview: { source: 'bandcamp'; id: string } | null;
};
export type SpotifyTrack = {
  id: string;
  title: string;
  artist: string;
  album: string;
  length_ms: number | null;
  added_at: number | null;
  status: SpotifyStatus;
  /** The library's track, where it has it. */
  track: { source: string; id: string } | null;
  /** Its album's. */
  release_group: string | null;
  request: AlbumRequest | null;
};
export type SpotifyPlaylist = { id: string; name: string; owner: string | null; collaborative: boolean; cover: string | null; of: number; have: number; status: SpotifyStatus };
export type SpotifyArtist = { id: string; name: string; photo: string | null; status: SpotifyStatus; have: number; artist: { id: string } | null };
export const getSpotify = () => request<SpotifyState>('/api/spotify');
/** The account's own Spotify app (its Client ID); empty forgets it, and disconnects. */
export const setSpotifyClient = (client_id: string) =>
  request<void>('/api/spotify/client', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ client_id }) });
/** Spotify's consent page, which comes back to `callback` (in the app, on to the app). */
export const connectSpotify = (callback: string, app: boolean) => post<{ url: string }>('/api/spotify/connect', { callback, app });
/** The live connection forgotten; an uploaded export's lists show again. */
export const disconnectSpotify = () => request<void>('/api/spotify', { method: 'DELETE' });
export const syncSpotify = () => post<SpotifyState>('/api/spotify/sync', {});
/** The uploaded export's lists forgotten (its listens stay, undone in Import). */
export const forgetSpotifyImport = () => request<void>('/api/spotify/import', { method: 'DELETE' });
export const getSpotifyAlbums = () => request<{ albums: SpotifyAlbum[] }>('/api/spotify/albums');
export const getSpotifyTracks = () => request<{ tracks: SpotifyTrack[] }>('/api/spotify/tracks');
export const getSpotifyPlaylists = () => request<{ playlists: SpotifyPlaylist[] }>('/api/spotify/playlists');
export const getSpotifyPlaylist = (id: string) => request<SpotifyPlaylist & { tracks: SpotifyTrack[] }>(`/api/spotify/playlists/${encodeURIComponent(id)}`);
export const getSpotifyArtists = () => request<{ artists: SpotifyArtist[] }>('/api/spotify/artists');
