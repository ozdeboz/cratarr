// The library on this device, and the one place that says where reads come from (lib/deviceIndexRule's `readsFrom`). No page asks
// whether it is offline: reads go through lib/api's `read`, and whatever must read again when they move is told (`whenReadsMove`).
//
// Memory is the device's (the tablet held 173 MB in rules): at rest an instance of index.wasm is held by nobody, and the bytes lie in
// IndexedDB. An index is opened for reads only while reads come from the device. `facts`, `Alike` and Radio's index are made only
// when a crate or a pick asks, and from one reading of the sounds (lib/deviceAnswers, over the instance opened here).
import indexUrl from './wasm/index.wasm?url';
import { accountKey, whenAccount, whoOf } from './account';
import { inApp } from './app.svelte';
import { lookedAgain, notReached, readsFrom, answered, type ReadsFrom, type Settle } from './deviceIndexRule';
import { artistPanel, FOUND_KINDS, foundView, listView, OWN, recordNumber, recordView } from './deviceReads';
import { instantiateIndex, type IndexEngine } from './indexEngine';
import { readBytes, readPart } from './indexStore';
import { lookForCratarr, reach, whenAway, whenNetwork, whenReached } from './reach.svelte';
import type { AlbumsView, AlbumView, ArtistView, SearchKind, ServerSearch } from './api';

export { readsFrom };

let settle: Settle = answered;
let looking: ReturnType<typeof setTimeout> | null = null;

/** Where reads come from now. */
export const readsNow = (): ReadsFrom => readsFrom({ unreachable: settle.failed === 2, noNetwork: reach.noNetwork });

/** Called when reads move, either way: what a page showed came from the other place. */
export const whenReadsMove = new Set<() => void>();

/** Where they come from as a page can watch it: what shows by it, a card dimmed, changes as they move. */
export const reads = $state({ from: readsNow().from });

function moved() {
  const now = readsNow().from;
  if (now === reads.from) return;
  reads.from = now;
  if (now === 'server') closeReads();
  for (const f of whenReadsMove) f();
}

function secondLook() {
  looking = null;
  void lookForCratarr().then((there) => {
    settle = lookedAgain(settle, there);
    moved();
  });
}

function away() {
  settle = notReached(settle, Date.now());
  if (settle.lookAt !== null && looking === null) looking = setTimeout(secondLook, Math.max(0, settle.lookAt - Date.now()));
  moved();
}

whenAway.add(away);
whenReached.add(() => {
  settle = answered;
  if (looking !== null) clearTimeout(looking);
  looking = null;
  moved();
});
whenNetwork.add(moved);
if (reach.away) away();

// --- index.wasm ---

let compiled: Promise<WebAssembly.Module> | null = null;

/** An instance of its own, the file compiled once. Failing, it is asked again next time. */
export function newEngine(): Promise<IndexEngine> {
  compiled ??= fetch(indexUrl)
    .then((r) => {
      if (!r.ok) throw new Error(`index.wasm: ${r.status}`);
      return r.arrayBuffer();
    })
    .then((bytes) => WebAssembly.compile(bytes))
    .catch((e) => {
      compiled = null;
      throw e;
    });
  return compiled.then(instantiateIndex);
}

// --- Reads ---

/** The index open for reads, and when it last stood where Cratarr does; what a read finds there. */
type Open = { engine: IndexEngine; at: number };
let reading: Promise<Open | null> | null = null;

/** Lets the instance go: its memory goes with it. */
export function closeReads() {
  reading = null;
}

whenAccount.add(closeReads);

/** Where the app gives what it keeps of the index, and when that last stood where Cratarr does (android's DeviceIndex). */
const APP_INDEX = '/__device/index';

/** What is kept of an account's index: in a browser its own, in the app the app's, read here by the same module. */
async function keptIndex(account: string): Promise<{ bytes: Uint8Array; at: number } | null> {
  if (!inApp) {
    const [kept, bytes] = await Promise.all([readPart(whoOf(account), 'index'), readBytes(whoOf(account), 'index')]);
    return bytes ? { bytes, at: kept?.at ?? 0 } : null;
  }
  const res = await fetch(APP_INDEX, { cache: 'no-store' });
  return res.ok ? { bytes: new Uint8Array(await res.arrayBuffer()), at: Number(res.headers.get('X-Cratarr-At')) } : null;
}

async function open(): Promise<Open | null> {
  const account = accountKey();
  if (account === null) return null;
  const kept = await keptIndex(account);
  if (!kept) return null;
  const engine = await newEngine();
  // Another account came meanwhile: what was opened is the one before's, and a page that asked is the new one's.
  return accountKey() !== account || engine.loadIndex(kept.bytes) ? null : { engine, at: kept.at };
}

function opened(): Promise<Open | null> {
  const asked = (reading ??= open().catch(() => null));
  // Nothing to read is no answer for good: a sync may bring it, and the file may come.
  void asked.then((held) => {
    if (!held && reading === asked) reading = null;
  });
  return asked;
}

/** The instance open for reads, for what asks more of it than a list. */
export const openReads = () => opened();

/** The Library's list from the index; null where it has none to give. */
export async function deviceAlbums(source?: string): Promise<AlbumsView | null> {
  if (source && source !== OWN) return null;
  const held = await opened();
  const rows = held?.engine.list();
  return held && rows ? listView(rows, accountKey(), held.at) : null;
}

/** What the words find in the index; null where it has none to ask. Other kinds asked are no failure: they are left out. */
export async function deviceSearch(q: string, kinds: SearchKind[], limit: number, offset: number, without: string[] = []): Promise<ServerSearch | null> {
  const asked = kinds.filter((k) => FOUND_KINDS.includes(k));
  const held = await opened();
  if (!held) return null;
  if (kinds.length && !asked.length) return { q, groups: {}, top: [] };
  const rows = held.engine.find({ q, kinds: asked, limit, offset, without });
  return rows && foundView(rows, q, offset);
}

/** An artist's panel from the index; null where nobody there goes by the name. */
export async function deviceArtist(source: string, name: string, without: string[] = []): Promise<ArtistView | null> {
  if (source !== OWN) return null;
  const rows = (await opened())?.engine.artist(name, without);
  return rows ? artistPanel(rows, name) : null;
}

/** A record's view from the index; null where it does not hold it. */
export async function deviceRecord(source: string, album: string): Promise<AlbumView | null> {
  const id = recordNumber(album);
  if (source !== OWN || id === null) return null;
  const rows = (await opened())?.engine.record(id);
  return rows ? recordView(rows) : null;
}
