// The Downloads shelf kept in this browser's IndexedDB (decision 0085, step 2; the owner, 2026-10-30: the page's own
// store in 0.12, moved into the Rust core's with step 4 in 0.13). It follows what the app (its Downloads, over the
// bridge) and an installed page (lib/offline) hold, and fills each album's details and a small cover in whenever Cratarr
// answers, one album at a time, quietly; Downloads reads it first, so it works with no network. The rules are lib/shelf.
import { untrack } from 'svelte';
import { albumCoverUrl, albumView, ApiError } from './api';
import { downloads, inApp } from './app.svelte';
import { installed, offline } from './offline.svelte';
import { reach, refusedFor, whenReached } from './reach.svelte';
import { coverKey, coverSize, filled, plan, toFill, type Held, type ShelfTrack, type ShelfWhere } from './shelf';

const DB = 'cratarr-shelf';
const TRACKS = 'tracks';
const COVERS = 'covers';

/** What the shelf holds: its records by key, and its covers as addresses to show (`source/album` → object URL). */
export const shelf = $state({ tracks: {} as Record<string, ShelfTrack>, covers: {} as Record<string, string>, ready: false });

/** The shelf's cover for an album where it has one (works with no network), else null. */
export const shelfCover = (source: string, album: string | null | undefined) => (album ? (shelf.covers[coverKey(source, album)] ?? null) : null);

let db: Promise<IDBDatabase | null> | null = null;
function open(): Promise<IDBDatabase | null> {
  db ??= new Promise((resolve) => {
    if (typeof indexedDB === 'undefined') return resolve(null);
    try {
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => {
        const d = req.result;
        if (!d.objectStoreNames.contains(TRACKS)) d.createObjectStore(TRACKS, { keyPath: 'key' });
        if (!d.objectStoreNames.contains(COVERS)) d.createObjectStore(COVERS, { keyPath: 'key' });
      };
      req.onsuccess = () => resolve(req.result);
      // A private window, or storage refused: the shelf lasts as long as the page.
      req.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
  return db;
}

const done = (t: IDBTransaction) =>
  new Promise<void>((resolve, reject) => {
    t.oncomplete = () => resolve();
    t.onerror = t.onabort = () => reject(t.error);
  });

async function all<T>(store: string): Promise<T[]> {
  const d = await open();
  if (!d) return [];
  return new Promise((resolve) => {
    const req = d.transaction(store).objectStore(store).getAll();
    req.onsuccess = () => resolve(req.result as T[]);
    req.onerror = () => resolve([]);
  });
}

async function write(put: ShelfTrack[], remove: string[]) {
  for (const r of put) shelf.tracks[r.key] = r;
  for (const k of remove) delete shelf.tracks[k];
  const d = await open();
  if (!d || (!put.length && !remove.length)) return;
  try {
    const t = d.transaction(TRACKS, 'readwrite');
    const s = t.objectStore(TRACKS);
    for (const r of put) s.put($state.snapshot(r));
    for (const k of remove) s.delete(k);
    await done(t);
  } catch {
    /* kept for this visit */
  }
}

type CoverRow = { key: string; blob: Blob; at: number };

async function keepCover(key: string, blob: Blob) {
  const was = shelf.covers[key];
  shelf.covers[key] = URL.createObjectURL(blob);
  if (was) URL.revokeObjectURL(was);
  const d = await open();
  if (!d) return;
  try {
    const t = d.transaction(COVERS, 'readwrite');
    t.objectStore(COVERS).put({ key, blob, at: Date.now() } satisfies CoverRow);
    await done(t);
  } catch {
    /* kept for this visit */
  }
}

/** Covers no record points at any more go. */
async function dropCovers() {
  const used = new Set(Object.values(shelf.tracks).flatMap((r) => (r.album_id ? [coverKey(r.source, r.album_id)] : [])));
  const gone = Object.keys(shelf.covers).filter((k) => !used.has(k));
  if (!gone.length) return;
  for (const k of gone) {
    URL.revokeObjectURL(shelf.covers[k]);
    delete shelf.covers[k];
  }
  const d = await open();
  if (!d) return;
  try {
    const t = d.transaction(COVERS, 'readwrite');
    for (const k of gone) t.objectStore(COVERS).delete(k);
    await done(t);
  } catch {
    /* tried again at the next change */
  }
}

/** A cover made small: COVER_PX on its longest side, WebP where this browser writes it, else JPEG. */
async function smallCover(url: string): Promise<Blob | null> {
  const r = await fetch(url);
  if (!r.ok) return null;
  const bmp = await createImageBitmap(await r.blob());
  const { w, h } = coverSize(bmp.width, bmp.height);
  const canvas = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(w, h) : Object.assign(document.createElement('canvas'), { width: w, height: h });
  const g = canvas.getContext('2d') as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!g) return null;
  g.drawImage(bmp, 0, 0, w, h);
  bmp.close();
  const as = async (type: string) =>
    canvas instanceof HTMLCanvasElement
      ? new Promise<Blob | null>((res) => canvas.toBlob(res, type, 0.8))
      : (canvas as OffscreenCanvas).convertToBlob({ type, quality: 0.8 });
  const webp = await as('image/webp');
  return webp?.type === 'image/webp' ? webp : as('image/jpeg');
}

/** What a place holds, followed: new tracks in, gone ones out. */
async function follow(where: ShelfWhere, held: Held[]) {
  const { put, remove } = plan(Object.values(shelf.tracks), where, held, Date.now());
  await write(put, remove);
  if (remove.length) await dropCovers();
  if (put.length) fill();
}

let filling = false;
/** Each album still to be read: its view, then its cover, one album at a time, while Cratarr answers. */
async function fill() {
  if (filling || !shelf.ready) return;
  filling = true;
  try {
    for (;;) {
      if (reach.away || refusedFor() > 0) return;
      const next = toFill(Object.values(shelf.tracks), Date.now())[0];
      if (!next) return;
      const { source, album_id } = next;
      try {
        const view = await albumView(source, album_id);
        await write(filled(Object.values(shelf.tracks), source, album_id, view, Date.now()), []);
      } catch (e) {
        // Not there (404): marked read, not asked again for a while. Anything else (out of reach, refused, a server
        // error): stopped until Cratarr answers again.
        if (!(e instanceof ApiError && e.status === 404)) return;
        const now = Date.now();
        await write(Object.values(shelf.tracks).filter((r) => r.source === source && r.album_id === album_id).map((r) => ({ ...r, filled: now })), []);
      }
      const key = coverKey(source, album_id);
      if (!shelf.covers[key]) {
        try {
          const blob = await smallCover(albumCoverUrl(source, album_id));
          if (blob) await keepCover(key, blob);
        } catch {
          /* no cover, or not now */
        }
      }
    }
  } finally {
    filling = false;
  }
}

const heldOfApp = (): Held[] => downloads.tracks.map(({ source, id, name, artist, album, album_id }) => ({ source, id, name, artist, album, album_id }));
const heldOfBrowser = (): Held[] =>
  Object.values(offline.kept).map(({ source, id, name, artist, album, album_id, duration_secs }) => ({ source, id, name, artist, album, album_id, duration_secs }));

if (typeof window !== 'undefined' && (inApp || installed)) {
  void (async () => {
    const [tracks, covers] = await Promise.all([all<ShelfTrack>(TRACKS), all<CoverRow>(COVERS)]);
    for (const r of tracks) shelf.tracks[r.key] = r;
    for (const c of covers) shelf.covers[c.key] = URL.createObjectURL(c.blob);
    shelf.ready = true;
    // What each place holds, followed as it changes (the app tells its downloads again at every change).
    $effect.root(() => {
      // Only what the place holds is followed; the shelf's own records are read untracked (writing them is no change).
      $effect(() => {
        if (!inApp || !downloads.known) return;
        const held = heldOfApp();
        untrack(() => void follow('app', held));
      });
      $effect(() => {
        if (!installed || inApp) return;
        const held = heldOfBrowser();
        untrack(() => void follow('browser', held));
      });
    });
    whenReached.add(() => void fill());
    void fill();
  })();
}
