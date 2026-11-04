// Scrobbling from the device (decision 0091; the owner, 2026-10-31: "scrobbling should indeed move to the device! This
// also makes more sense privacy wise"): the player that sounded a play tells ListenBrainz and Last.fm itself, with this
// device's own sign-ins, never sent to Cratarr's server. A browser's page does it here; the apps do the same in their
// own code (they play with the page closed). Pure but for the `fetch` it is given.
import SparkMD5 from 'spark-md5';

/** A play as it is scrobbled: its id (decision 0087: one play is sent once, whichever way), when (epoch s). */
export type ScrobblePlay = {
  id: string;
  at: number;
  artist: string;
  title: string;
  album: string | null;
  album_artist?: string | null;
  duration_secs?: number | null;
  mbid?: string | null;
};
export type Service = 'listenbrainz' | 'lastfm';

/** Last.fm takes a scrobble at most this long after it was played (14 days); ListenBrainz any time. */
export const LASTFM_MOST_S = 14 * 86_400;
/** How many go in one call: Last.fm's limit, and ListenBrainz's own (an import of up to 1000). */
export const BATCH = { lastfm: 50, listenbrainz: 1000 } as const;

const LB = 'https://api.listenbrainz.org/1';
const LFM = 'https://ws.audioscrobbler.com/2.0/';

/** A service's refusal: `signedOut` when it no longer takes the sign-in (its keys are asked for again). */
export class ScrobbleError extends Error {
  readonly signedOut: boolean;
  constructor(message: string, signedOut = false) {
    super(message);
    this.signedOut = signedOut;
  }
}

// ListenBrainz: the user token from its settings page.

/** Whose the token is; a ScrobbleError where it is not one. */
export async function listenBrainzUser(token: string, f: typeof fetch = fetch): Promise<string> {
  const r = await f(`${LB}/validate-token`, { headers: { Authorization: `Token ${token}` } });
  const b = (await r.json().catch(() => ({}))) as { valid?: boolean; user_name?: string; message?: string };
  if (!r.ok || !b.valid || !b.user_name) throw new ScrobbleError(b.message || 'ListenBrainz does not know this token', true);
  return b.user_name;
}

const lbListen = (p: ScrobblePlay, at: boolean) => ({
  ...(at ? { listened_at: p.at } : {}),
  track_metadata: {
    artist_name: p.artist,
    track_name: p.title,
    ...(p.album ? { release_name: p.album } : {}),
    additional_info: {
      submission_client: 'Cratarr',
      ...(p.duration_secs ? { duration_ms: Math.round(p.duration_secs * 1000) } : {}),
      ...(p.mbid ? { recording_mbid: p.mbid } : {}),
      ...(p.album_artist ? { release_artist_name: p.album_artist } : {}),
    },
  },
});

async function lbSubmit(token: string, body: unknown, f: typeof fetch) {
  const r = await f(`${LB}/submit-listens`, { method: 'POST', headers: { Authorization: `Token ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  if (r.ok) return;
  const b = (await r.json().catch(() => ({}))) as { error?: string };
  throw new ScrobbleError(b.error || `ListenBrainz refused it (${r.status})`, r.status === 401);
}
export const listenBrainzScrobble = (token: string, plays: ScrobblePlay[], f: typeof fetch = fetch) =>
  lbSubmit(token, { listen_type: plays.length === 1 ? 'single' : 'import', payload: plays.map((p) => lbListen(p, true)) }, f);
export const listenBrainzPlaying = (token: string, p: ScrobblePlay, f: typeof fetch = fetch) => lbSubmit(token, { listen_type: 'playing_now', payload: [lbListen(p, false)] }, f);

// Last.fm: the person's own API key and secret (Cratarr ships none), its web sign-in, the session key traded for.

/** A call's signature: every parameter but `format` and `callback`, sorted by name, each name then value, the secret
 *  last, MD5'd (Last.fm's). */
export function lastFmSignature(params: Record<string, string>, secret: string): string {
  const text = Object.keys(params)
    .filter((k) => k !== 'format' && k !== 'callback')
    .sort()
    .map((k) => `${k}${params[k]}`)
    .join('');
  return SparkMD5.hash(text + secret);
}

/** Last.fm's sign-in page, coming back to `callback` with a token. */
export const lastFmAuthUrl = (key: string, callback: string) => `https://www.last.fm/api/auth/?api_key=${encodeURIComponent(key)}&cb=${encodeURIComponent(callback)}`;

async function lfmCall(method: string, params: Record<string, string>, key: string, secret: string, f: typeof fetch): Promise<Record<string, unknown>> {
  const all = { ...params, method, api_key: key };
  const body = new URLSearchParams({ ...all, api_sig: lastFmSignature(all, secret), format: 'json' });
  const r = await f(LFM, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: body.toString() });
  const b = (await r.json().catch(() => ({}))) as Record<string, unknown> & { error?: number; message?: string };
  // 4, 9, 10, 14, 26: authentication, a session or a key no longer good; the rest a refusal of this call.
  if (b.error) throw new ScrobbleError(b.message || `Last.fm refused it (${b.error})`, [4, 9, 10, 14, 26].includes(b.error));
  if (!r.ok) throw new ScrobbleError(`Last.fm refused it (${r.status})`);
  return b;
}

/** The session the sign-in's token trades for: its key and whose it is. */
export async function lastFmSession(key: string, secret: string, token: string, f: typeof fetch = fetch): Promise<{ session: string; name: string }> {
  const b = (await lfmCall('auth.getSession', { token }, key, secret, f)) as { session?: { key?: string; name?: string } };
  if (!b.session?.key) throw new ScrobbleError('Last.fm gave no session', true);
  return { session: b.session.key, name: b.session.name ?? '' };
}

export async function lastFmScrobble(key: string, secret: string, session: string, plays: ScrobblePlay[], f: typeof fetch = fetch) {
  const params: Record<string, string> = { sk: session };
  plays.forEach((p, i) => {
    params[`artist[${i}]`] = p.artist;
    params[`track[${i}]`] = p.title;
    params[`timestamp[${i}]`] = String(p.at);
    if (p.album) params[`album[${i}]`] = p.album;
    if (p.album_artist) params[`albumArtist[${i}]`] = p.album_artist;
    if (p.duration_secs) params[`duration[${i}]`] = String(Math.round(p.duration_secs));
    if (p.mbid) params[`mbid[${i}]`] = p.mbid;
  });
  await lfmCall('track.scrobble', params, key, secret, f);
}
export async function lastFmPlaying(key: string, secret: string, session: string, p: ScrobblePlay, f: typeof fetch = fetch) {
  const params: Record<string, string> = { sk: session, artist: p.artist, track: p.title };
  if (p.album) params.album = p.album;
  if (p.duration_secs) params.duration = String(Math.round(p.duration_secs));
  await lfmCall('track.updateNowPlaying', params, key, secret, f);
}

// The queue: what is still to be sent to each service.

/** What a service is sent next of the queue (`sent`: the ids it has), at most a batch: Last.fm's within its 14 days. */
export function nextBatch(queue: readonly ScrobblePlay[], service: Service, sent: ReadonlySet<string>, now: number): ScrobblePlay[] {
  return queue.filter((p) => !sent.has(p.id) && (service !== 'lastfm' || now - p.at <= LASTFM_MOST_S)).slice(0, BATCH[service]);
}

/** The queue less what every service took or can no longer take (Last.fm's older than 14 days, where ListenBrainz
 *  has it or is not signed in to). */
export function remaining(queue: readonly ScrobblePlay[], services: readonly Service[], sent: Readonly<Record<Service, ReadonlySet<string>>>, now: number): ScrobblePlay[] {
  return queue.filter((p) => services.some((s) => !sent[s].has(p.id) && (s !== 'lastfm' || now - p.at <= LASTFM_MOST_S)));
}
