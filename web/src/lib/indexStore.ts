// What this device keeps of the library, each account's apart (decision 0101: `cratarr-index@name`, hidden while another is signed in
// and there at its next sign-in), in this browser's IndexedDB. Only bytes, as the rules crate saves them (the owner, 2026-11-06: one
// packed format, the store only bytes), and how each part stands. A part's bytes and its standing are written in one transaction, so
// a page killed halfway keeps the old pair and fetches again, never bytes that say one rev under a word that says another.
import type { Part } from './indexEngine';

const DB = 'cratarr-index';
const PARTS = 'parts';

/** The parts a device keeps. D12's fits and D13's small covers are more of these, each with its own rev, as the sounds are. */
export type PartName = 'index' | 'sounds';

/** A part as it is kept. `at`: when it last stood where Cratarr does (ms); `wants`: the version the page that wrote this reads; `part`:
 *  how its fetching stands. */
export type Kept = { bytes: Uint8Array | null; rev: number | null; at: number | null; wants: number | null; part: Omit<Part, 'started'> };

const open = new Map<string, Promise<IDBDatabase | null>>();

function db(who: string): Promise<IDBDatabase | null> {
  let d = open.get(who);
  if (!d) {
    d = new Promise((done) => {
      try {
        if (typeof indexedDB === 'undefined') return done(null);
        const r = indexedDB.open(DB + who, 1);
        r.onupgradeneeded = () => r.result.createObjectStore(PARTS);
        r.onsuccess = () => done(r.result);
        r.onerror = () => done(null);
        r.onblocked = () => done(null);
      } catch {
        done(null);
      }
    });
    open.set(who, d);
  }
  return d;
}

/** A part as kept for `who` (lib/account's whoOf); undefined where nothing is, or it cannot be read. */
export async function readPart(who: string, name: PartName): Promise<Kept | undefined> {
  const d = await db(who);
  if (!d) return undefined;
  return new Promise((done) => {
    try {
      const r = d.transaction(PARTS, 'readonly').objectStore(PARTS).get(name);
      r.onsuccess = () => done(r.result as Kept | undefined);
      r.onerror = () => done(undefined);
    } catch {
      done(undefined);
    }
  });
}

/** Keeps a part for `who`; whether it was written. A private window or storage refused keeps nothing, and the page says so by
 *  having no date to show. */
export async function writePart(who: string, name: PartName, kept: Kept): Promise<boolean> {
  const d = await db(who);
  if (!d) return false;
  return new Promise((done) => {
    try {
      const t = d.transaction(PARTS, 'readwrite');
      t.objectStore(PARTS).put(kept, name);
      t.oncomplete = () => done(true);
      t.onerror = () => done(false);
      t.onabort = () => done(false);
    } catch {
      done(false);
    }
  });
}
