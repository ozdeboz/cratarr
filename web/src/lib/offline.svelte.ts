// Downloads in the installed PWA (0.9.8, the owner: "offline in the PWA"): tracks kept in this browser's own storage,
// played without Cratarr, as the app keeps them (decision 0038). Cratarr makes each track's stream once (the server's
// POST /api/offline/prepare, form "stream"): the same bytes the player would stream from its start, kept on the server
// until fetched. This page fetches it whole (?whole=1) into the Cache Storage, with its cover and waveform, and keeps a
// list of what it holds. The service worker (public/sw.js) answers the player's stream request for a kept track from
// that copy, whatever position it asks for: the player cuts what lies before it (its append window). Only where the page
// is installed (a browser tab's storage is for the browser to drop), and never Bandcamp's previews (relays).
// Each account keeps its own list (decision 0101), hidden while another is signed in; a stream two accounts keep is
// stored once, and goes with the last one's.
import { accountKey, perAccount, whenAccount, whoOf } from './account';
import { desktop } from './desktop';
import { losslessAs, playsSmaller } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';

/** The cache the service worker reads kept streams from (everyone's); the covers' and data's are each account's. Their
 *  names are public/sw.js's: raised there, here too, and the list of what is held with them, or it would list what the
 *  worker let go. */
const OFFLINE = 'cratarr-offline-2';
const COVERS = 'cratarr-covers-2';
const DATA = 'cratarr-data-2';
const INDEX = 'cratarr-offline-index-2';
const OLD_INDEX = 'cratarr-offline-index';

/** A kept track: what the queue and Downloads show of it, and how much it holds. */
export type KeptTrack = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  duration_secs: number | null;
  bytes: number;
  at: number;
};
/** A track on its way: waiting for Cratarr to make its copy, coming in, or failed (why, in a line). */
export type Fetching = { state: 'waiting' | 'downloading' | 'failed'; percent?: number; why?: string };

/** `who`: whose list it is (lib/account's whoOf). */
export const offline = $state({ kept: {} as Record<string, KeptTrack>, fetching: {} as Record<string, Fetching>, who: whoOf(accountKey()) });

/** Installed (on a home screen, as an app window, or in the desktop app): where this page keeps tracks. */
export const installed =
  typeof window !== 'undefined' &&
  (matchMedia('(display-mode: standalone)').matches ||
    (navigator as Navigator & { standalone?: boolean }).standalone === true ||
    !!desktop);
export const keyOf = (t: { source: string; id: string }) => `${t.source}/${t.id}`;
/** The stream's address as the service worker finds it: the track alone, whatever form or position is asked. */
export const streamKey = (t: { source: string; id: string }) => `/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}`;
export const isKept = (t: { source: string; id: string }) => !!offline.kept[keyOf(t)];

try {
  localStorage.removeItem(OLD_INDEX);
} catch {
  /* no storage */
}
/** Whose list [offline] holds: saved under them, even while another's comes in. */
let listOf = perAccount(INDEX);
function load() {
  listOf = perAccount(INDEX);
  offline.who = whoOf(accountKey());
  offline.kept = {};
  offline.fetching = {};
  try {
    const saved = JSON.parse(localStorage.getItem(listOf) ?? '{}');
    if (saved && typeof saved === 'object') offline.kept = saved;
  } catch {
    /* none kept, or no storage */
  }
}
load();
whenAccount.add(() => {
  if (perAccount(INDEX) !== listOf) load();
});
function save() {
  try {
    localStorage.setItem(listOf, JSON.stringify(offline.kept));
  } catch {
    /* kept for this visit */
  }
}
/** Whether another account's list here holds the track too (its stream then stays). */
function keptByAnother(key: string): boolean {
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const name = localStorage.key(i);
      if (!name || name === listOf || !name.startsWith(INDEX)) continue;
      if (key in (JSON.parse(localStorage.getItem(name) ?? '{}') ?? {})) return true;
    }
  } catch {
    /* none read: kept as it is */
    return true;
  }
  return false;
}

type Wanted = { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null };
type Prepared = { source: string; id: string; offline: boolean; error?: string; file?: { url: string; type: string; ready: boolean; bytes: number | null } };
type Facts = Prepared & { cover?: string; waveform?: string; track?: { duration?: number | null } };

const post = async <T>(path: string, body: unknown): Promise<T> => {
  const r = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  if (!r.ok) throw new Error(r.status === 404 ? 'Cratarr cannot keep tracks for this browser yet.' : `Cratarr said ${r.status}.`);
  return r.json();
};
/** The form asked of Cratarr: the smaller copy this browser plays (Opus, else AAC) when Sync's Quality says so, else
 *  the original where this browser plays it (where it does not, the smaller copy all the same). */
function chosen(): 'opus' | 'alac' | 'mp3' | 'original' {
  if (prefs.offlineQuality === 'smaller' && playsSmaller() === 'opus') return 'opus';
  return losslessAs() ?? 'original';
}
const form = () => ({ as: 'stream', form: chosen(), opus: chosen() === 'opus' });

/** Keeps `tracks` in this browser: Cratarr makes each one's stream, then it comes in whole, its cover and waveform too. */
export async function keepOffline(tracks: Wanted[]) {
  // The account they are fetched for: one signed in meanwhile gets none of them.
  const forList = listOf;
  const want = tracks.filter((t) => !isKept(t) && !offline.fetching[keyOf(t)]?.state.match(/waiting|downloading/));
  if (!want.length) return;
  for (const t of want) offline.fetching[keyOf(t)] = { state: 'waiting' };
  const ask = want.map(({ source, id }) => ({ source, id }));
  try {
    // Until every copy is made (Cratarr makes them on the first ask): asked again every few seconds.
    let ready: Prepared[] = [];
    for (let round = 0; round < 120; round++) {
      const p = await post<{ tracks: Prepared[] }>('/api/offline/prepare', { tracks: ask, ...form() });
      for (const x of p.tracks) if (!x.offline) offline.fetching[keyOf(x)] = { state: 'failed', why: x.error ?? 'It cannot be kept (a preview).' };
      ready = p.tracks.filter((x) => x.offline);
      if (ready.every((x) => x.file?.ready)) break;
      await new Promise((r) => setTimeout(r, 3000));
    }
    const facts = await post<{ tracks: Facts[] }>('/api/offline', { tracks: ready.map(({ source, id }) => ({ source, id })), ...form() });
    const byKey = new Map(facts.tracks.map((f) => [keyOf(f), f]));
    for (const t of want) {
      const f = byKey.get(keyOf(t));
      if (!f?.offline || !f.file?.ready) continue;
      await fetchOne(t, f, forList);
    }
  } catch (e) {
    for (const t of want) if (offline.fetching[keyOf(t)]?.state !== 'failed' && !isKept(t)) offline.fetching[keyOf(t)] = { state: 'failed', why: e instanceof Error ? e.message : String(e) };
  }
}

async function fetchOne(t: Wanted, f: Facts, forList: string) {
  if (forList !== listOf) return;
  const key = keyOf(t);
  offline.fetching[key] = { state: 'downloading', percent: 0 };
  try {
    const res = await fetch(f.file!.url);
    if (!res.ok || !res.body) throw new Error(`Cratarr said ${res.status}.`);
    const total = Number(res.headers.get('content-length')) || f.file!.bytes || 0;
    // Read whole, counting, then kept with the stream's own headers (the player reads its sample rate and length).
    const reader = res.body.getReader();
    const parts: Uint8Array[] = [];
    let got = 0;
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      parts.push(value);
      got += value.length;
      if (total) offline.fetching[key] = { state: 'downloading', percent: Math.round((got / total) * 100) };
    }
    const headers = new Headers(res.headers);
    headers.set('content-length', String(got));
    // Before it is stored: a stream in no list is in nobody's way to let go of, for good.
    if (forList !== listOf) return;
    await (await caches.open(OFFLINE)).put(streamKey(t), new Response(new Blob(parts as BlobPart[], { type: res.headers.get('content-type') ?? '' }), { status: 200, headers }));
    if (forList !== listOf) return;
    // Its cover and waveform where the service worker keeps them already, the account's.
    if (f.cover) await keepBeside(perAccount(COVERS), f.cover, new URL(f.cover, location.origin).pathname);
    if (f.waveform) await keepBeside(perAccount(DATA), f.waveform, f.waveform);
    offline.kept[key] = {
      source: t.source,
      id: t.id,
      name: t.name,
      artist: t.artist,
      album: t.album,
      album_id: t.album_id,
      duration_secs: f.track?.duration ?? null,
      bytes: got,
      at: Date.now(),
    };
    save();
    delete offline.fetching[key];
  } catch (e) {
    offline.fetching[key] = { state: 'failed', why: e instanceof Error ? e.message : String(e) };
  }
}

async function keepBeside(cache: string, url: string, key: string) {
  try {
    const r = await fetch(url);
    if (r.ok) await (await caches.open(cache)).put(key, r);
  } catch {
    /* the track plays without it */
  }
}

/** Takes these tracks out of the account's list here; a stream another account keeps stays. */
export async function forgetOffline(tracks: { source: string; id: string }[]) {
  const c = await caches.open(OFFLINE);
  for (const t of tracks) {
    if (!keptByAnother(keyOf(t))) await c.delete(streamKey(t));
    delete offline.kept[keyOf(t)];
    delete offline.fetching[keyOf(t)];
  }
  save();
}

/** What this browser holds, in bytes. */
export const keptBytes = () => Object.values(offline.kept).reduce((n, t) => n + t.bytes, 0);
