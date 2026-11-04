// Scrobbling, the account's way (decision 0091): from each device (the start) or through the server. This device's
// own sign-ins and, in a browser, its queue and its sending (lib/scrobbleDevice); in the apps, their own code does the
// sending with the sign-ins the app keeps (the page asks it: `scrobblers`). Never sent to Cratarr's server.
import { getScrobbles } from './api';
import { inApp, onApp, toApp } from './app.svelte';
import { mine, setScrobbleWay } from './mySettings.svelte';
import {
  lastFmPlaying,
  lastFmScrobble,
  lastFmSession,
  listenBrainzPlaying,
  listenBrainzScrobble,
  listenBrainzUser,
  nextBatch,
  remaining,
  ScrobbleError,
  type ScrobblePlay,
  type Service,
} from './scrobbleDevice';

export type Way = 'device' | 'server';
/** This device's sign-ins, as the page may know them: whose, and whether it still takes them (`broken`: why not). */
export type HereService = { name: string; broken: string | null };

export const scrobbling = $state({
  /** The account's way, as the server last said; null until it has. */
  way: null as Way | null,
  here: { listenbrainz: null as HereService | null, lastfm: null as HereService | null },
  /** Plays still to go from this device. */
  queued: 0,
});

/** The way read again (and, through the server, its services' state is the Listening services' own). */
// Read once the page has an account, and what waits from before sent then.
if (typeof window !== 'undefined')
  queueMicrotask(() =>
    import('./account').then(({ whenAccount }) =>
      whenAccount.add(() => {
        void readWay().then(send);
        readHere();
      }),
    ),
  );
export async function readWay() {
  try {
    const r = (await getScrobbles()) as { way?: Way };
    scrobbling.way = r.way === 'device' ? 'device' : 'server';
  } catch {
    // As it was.
  }
}
/** The account's choice, kept (the server sends what it holds first, then lets the keys go, when it goes to `device`). */
export async function setWay(way: Way) {
  scrobbling.way = way;
  await setScrobbleWay(way);
  await readWay();
}

// The sign-ins: the app's, or this browser's own storage.
type Kept = { listenbrainz?: { token: string; name: string; broken?: string | null }; lastfm?: { key: string; secret: string; session: string; name: string; broken?: string | null } };
const KEPT = 'cratarr.scrobblers';
const QUEUE = 'cratarr.scrobbleQueue';
function read<T>(k: string, none: T): T {
  try {
    const v = localStorage.getItem(k);
    return v ? (JSON.parse(v) as T) : none;
  } catch {
    return none;
  }
}
function write(k: string, v: unknown) {
  try {
    localStorage.setItem(k, JSON.stringify(v));
  } catch {
    // Kept for this page only.
  }
}
function shown(k: Kept) {
  scrobbling.here = {
    listenbrainz: k.listenbrainz ? { name: k.listenbrainz.name, broken: k.listenbrainz.broken ?? null } : null,
    lastfm: k.lastfm ? { name: k.lastfm.name, broken: k.lastfm.broken ?? null } : null,
  };
}
// The app sends from its own player: it learns the way and the tracking as they are read or change here.
if (inApp)
  $effect.root(() => {
    $effect(() => {
      const way = scrobbling.way;
      const tracking = mine.tracking;
      if (way) toApp({ type: 'scrobblers', op: 'way', way, tracking });
    });
  });
if (inApp) onApp('scrobblers', (m) => ((scrobbling.here = (m.here as typeof scrobbling.here) ?? { listenbrainz: null, lastfm: null }), (scrobbling.queued = Number(m.queued) || 0)));

/** This device's sign-ins read (the app's, or this browser's). */
export function readHere() {
  if (inApp) return toApp({ type: 'scrobblers', op: 'list' });
  shown(read<Kept>(KEPT, {}));
  scrobbling.queued = read<Queued>(QUEUE, { plays: [], sent: { listenbrainz: [], lastfm: [] } }).plays.length;
}

/** ListenBrainz signed in to with its user token (checked first). */
export async function keepListenBrainz(token: string) {
  const name = await listenBrainzUser(token);
  if (inApp) return toApp({ type: 'scrobblers', op: 'keep', service: 'listenbrainz', token, name });
  write(KEPT, { ...read<Kept>(KEPT, {}), listenbrainz: { token, name } });
  readHere();
}
/** Last.fm: the key and secret kept (for the session to come), then its sign-in page, coming back with a token. In the
 *  app, back through its own link (`cratarr://connected/lastfm-device`), the app then trading it for the session. */
export function signInLastFm(key: string, secret: string) {
  if (inApp) return toApp({ type: 'scrobblers', op: 'lastfm', key, secret });
  write('cratarr.lastfmPending', { key, secret });
  const back = `${location.origin}${location.pathname}#/settings?view=${encodeURIComponent('settings:tracking/lastfm')}`;
  location.assign(`https://www.last.fm/api/auth/?api_key=${encodeURIComponent(key)}&cb=${encodeURIComponent(back)}`);
}
/** Back from Last.fm's sign-in in a browser: its token in the address, traded for the session. */
export async function lastFmBack(): Promise<string | null> {
  if (inApp) return null;
  const token = /[?&]token=([^&#]+)/.exec(location.href)?.[1];
  const pending = read<{ key: string; secret: string } | null>('cratarr.lastfmPending', null);
  if (!token || !pending) return null;
  try {
    const { session, name } = await lastFmSession(pending.key, pending.secret, decodeURIComponent(token));
    write(KEPT, { ...read<Kept>(KEPT, {}), lastfm: { key: pending.key, secret: pending.secret, session, name } });
    readHere();
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : 'Last.fm refused the sign-in';
  } finally {
    try {
      localStorage.removeItem('cratarr.lastfmPending');
    } catch {
      // Gone with the page.
    }
    history.replaceState(history.state, '', location.href.replace(/[?&]token=[^&#]+/, ''));
  }
}
export function forgetHere(service: Service) {
  if (inApp) return toApp({ type: 'scrobblers', op: 'forget', service });
  const k = read<Kept>(KEPT, {});
  delete k[service];
  write(KEPT, k);
  readHere();
}

// A browser's own sending: its plays, queued with their ids, sent to each service signed in to here.
type Queued = { plays: ScrobblePlay[]; sent: Record<Service, string[]> };
let sending = false;
const now = () => Math.floor(Date.now() / 1000);

/** A play this browser's player sounded and counted (decision 0022), its id that of its report to Cratarr. */
export function playedHere(p: ScrobblePlay) {
  if (inApp || scrobbling.way !== 'device' || !mine.tracking) return;
  const q = read<Queued>(QUEUE, { plays: [], sent: { listenbrainz: [], lastfm: [] } });
  if (q.plays.some((x) => x.id === p.id)) return;
  q.plays.push(p);
  write(QUEUE, q);
  void send();
}
/** What starts playing here, as now playing; best effort, never queued. */
export function playingHere(p: ScrobblePlay) {
  if (inApp || scrobbling.way !== 'device' || !mine.tracking) return;
  const k = read<Kept>(KEPT, {});
  if (k.listenbrainz && !k.listenbrainz.broken) listenBrainzPlaying(k.listenbrainz.token, p).catch(() => {});
  if (k.lastfm && !k.lastfm.broken) lastFmPlaying(k.lastfm.key, k.lastfm.secret, k.lastfm.session, p).catch(() => {});
}

/** The queue sent, each service its batches; a service refusing its sign-in stops until signed in again (said in
 *  Tracking); one down gets them later (the next play, or the page opened again). Tracking off: nothing sent, the
 *  queue dropped (decision 0091: what was queued then is never sent later). */
export async function send() {
  if (inApp || sending) return;
  if (!mine.tracking) return write(QUEUE, { plays: [], sent: { listenbrainz: [], lastfm: [] } });
  sending = true;
  try {
    const k = read<Kept>(KEPT, {});
    const q = read<Queued>(QUEUE, { plays: [], sent: { listenbrainz: [], lastfm: [] } });
    const services = (['listenbrainz', 'lastfm'] as Service[]).filter((s) => k[s] && !k[s]!.broken);
    for (const s of services) {
      for (;;) {
        const batch = nextBatch(q.plays, s, new Set(q.sent[s]), now());
        if (!batch.length) break;
        try {
          if (s === 'listenbrainz') await listenBrainzScrobble(k.listenbrainz!.token, batch);
          else await lastFmScrobble(k.lastfm!.key, k.lastfm!.secret, k.lastfm!.session, batch);
          q.sent[s] = [...q.sent[s], ...batch.map((p) => p.id)];
        } catch (e) {
          if (e instanceof ScrobbleError && e.signedOut) (k[s]!.broken = e.message), write(KEPT, k);
          break;
        }
      }
    }
    const sent = { listenbrainz: new Set(q.sent.listenbrainz), lastfm: new Set(q.sent.lastfm) };
    const left = remaining(q.plays, services, sent, now());
    const ids = new Set(left.map((p) => p.id));
    write(QUEUE, { plays: left, sent: { listenbrainz: q.sent.listenbrainz.filter((id) => ids.has(id)), lastfm: q.sent.lastfm.filter((id) => ids.has(id)) } });
    readHere();
  } finally {
    sending = false;
  }
}
