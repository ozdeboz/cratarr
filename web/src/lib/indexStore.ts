// What this device keeps of the library, each account's apart (decision 0101: `cratarr-index@name`, hidden while another is signed in
// and there at its next sign-in), in this browser's IndexedDB. Only bytes, as the rules crate saves them (the owner, 2026-11-06: one
// packed format, the store only bytes), and how each part stands. A part's bytes and its standing are written in one transaction, so
// a page killed halfway keeps the old pair and fetches again, never bytes that say one rev under a word that says another. They lie
// in rows of their own, as a look reads how a part stands far more often than it reads megabytes. Small covers are a row each.
import type { CoverFrame } from './coverFrames';
import type { Part } from './indexEngine';

const DB = 'cratarr-index';
const PARTS = 'parts';
const COVERS = 'covers';

/** The parts a device keeps, each with its own rev. An account's own has none: it goes by when it was taken. */
export type PartName = 'index' | 'sounds' | 'covers' | 'own';

/** A part as it is kept. `at`: when it last stood where Cratarr does (ms); `wants`: the version the page that wrote this reads; `part`:
 *  how its fetching stands. */
export type Kept = { rev: number | null; at: number | null; wants: number | null; part: Omit<Part, 'started'> };

const bytesOf = (name: PartName) => `${name}:bytes`;

const open = new Map<string, Promise<IDBDatabase | null>>();

/** The account's database, opened once and kept while it stands. An open that failed, or that another tab's connection blocked, is
 *  not kept: the next read or write asks again (audit/OFFLINE-REVIEW.md, finding 6). A connection another tab's upgrade needs is
 *  let go, and opened anew when next asked; one that opens after this page stopped waiting is closed, so it blocks nobody. */
function db(who: string): Promise<IDBDatabase | null> {
  let d = open.get(who);
  if (!d) {
    const opening: Promise<IDBDatabase | null> = new Promise((done) => {
      try {
        if (typeof indexedDB === 'undefined') return done(null);
        let given = false;
        const r = indexedDB.open(DB + who, 2);
        r.onupgradeneeded = () => {
          for (const store of [PARTS, COVERS]) if (!r.result.objectStoreNames.contains(store)) r.result.createObjectStore(store);
        };
        r.onsuccess = () => {
          const got = r.result;
          if (given) return got.close();
          given = true;
          got.onversionchange = () => {
            got.close();
            if (open.get(who) === opening) open.delete(who);
          };
          done(got);
        };
        r.onerror = () => {
          given = true;
          done(null);
        };
        r.onblocked = () => {
          given = true;
          done(null);
        };
      } catch {
        done(null);
      }
    });
    d = opening;
    open.set(who, d);
    void d.then((got) => {
      if (!got && open.get(who) === opening) open.delete(who);
    });
  }
  return d;
}

async function get<T>(who: string, key: string): Promise<T | undefined> {
  const d = await db(who);
  if (!d) return undefined;
  return new Promise((done) => {
    try {
      const r = d.transaction(PARTS, 'readonly').objectStore(PARTS).get(key);
      r.onsuccess = () => done(r.result as T | undefined);
      r.onerror = () => done(undefined);
    } catch {
      done(undefined);
    }
  });
}

/** `work` in one transaction of `store`; whether it was written. A private window or storage refused keeps nothing, and the page says
 *  so by having no date to show. */
async function written(who: string, store: string, work: (s: IDBObjectStore) => void): Promise<boolean> {
  const d = await db(who);
  if (!d) return false;
  return new Promise((done) => {
    try {
      const t = d.transaction(store, 'readwrite');
      work(t.objectStore(store));
      t.oncomplete = () => done(true);
      t.onerror = () => done(false);
      t.onabort = () => done(false);
    } catch {
      done(false);
    }
  });
}

/** How a part stands for `who` (lib/account's whoOf); undefined where nothing is, or it cannot be read. */
export const readPart = (who: string, name: PartName) => get<Kept>(who, name);

/** A part's bytes. */
export const readBytes = (who: string, name: PartName) => get<Uint8Array>(who, bytesOf(name));

/** Keeps how a part stands for `who`, and new `bytes` if it has any; whether it was written. */
export const writePart = (who: string, name: PartName, kept: Kept, bytes?: Uint8Array) =>
  written(who, PARTS, (s) => {
    s.put(kept, name);
    if (bytes) s.put(bytes, bytesOf(name));
  });

/** Small covers for `who`: each record's taken, or let go where it has none now. */
export const putCovers = (who: string, frames: CoverFrame[]) =>
  frames.length === 0
    ? Promise.resolve(true)
    : written(who, COVERS, (s) => {
        for (const f of frames) {
          if (f.jpeg) s.put(f.jpeg, f.record);
          else s.delete(f.record);
        }
      });

/** Small covers for `who` kept only for `records`: once a whole is all in, what it no longer holds goes. A whole cut short never
 *  gets here, so what was held stays until one is whole (audit/OFFLINE-REVIEW.md, finding 4). */
export async function keepCoversOf(who: string, records: Set<number>): Promise<boolean> {
  const d = await db(who);
  if (!d) return false;
  const keys = await new Promise<IDBValidKey[] | null>((done) => {
    try {
      const r = d.transaction(COVERS, 'readonly').objectStore(COVERS).getAllKeys();
      r.onsuccess = () => done(r.result);
      r.onerror = () => done(null);
    } catch {
      done(null);
    }
  });
  if (!keys) return false;
  const gone = keys.filter((k) => !records.has(Number(k)));
  return gone.length === 0 || written(who, COVERS, (s) => gone.forEach((k) => s.delete(k)));
}
