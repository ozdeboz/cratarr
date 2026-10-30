// What this device keeps for Cratarr while it may be out of reach (decision 0085, step 3): the outbox, the plays not yet
// told, the change log's last rev. In IndexedDB (lasts, and is not capped at a few MB); where a browser refuses it (a
// private window, storage blocked), in localStorage; where both are refused, for as long as the page is open.

const DB = 'cratarr-kept';
const STORE = 'kept';
const LS = (name: string) => `cratarr-kept-${name}`;

let opened: Promise<IDBDatabase | null> | null = null;
function db(): Promise<IDBDatabase | null> {
  if (!opened)
    opened = new Promise((done) => {
      try {
        if (typeof indexedDB === 'undefined') return done(null);
        const r = indexedDB.open(DB, 1);
        r.onupgradeneeded = () => r.result.createObjectStore(STORE);
        r.onsuccess = () => done(r.result);
        r.onerror = () => done(null);
        r.onblocked = () => done(null);
      } catch {
        done(null);
      }
    });
  return opened;
}

/** What was kept under `name`; undefined when nothing was (or it cannot be read). */
export async function readKept<T>(name: string): Promise<T | undefined> {
  const d = await db();
  if (d) {
    const v = await new Promise<T | undefined>((done) => {
      try {
        const r = d.transaction(STORE, 'readonly').objectStore(STORE).get(name);
        r.onsuccess = () => done(r.result as T | undefined);
        r.onerror = () => done(undefined);
      } catch {
        done(undefined);
      }
    });
    if (v !== undefined) return v;
  }
  try {
    const s = localStorage.getItem(LS(name));
    return s === null ? undefined : (JSON.parse(s) as T);
  } catch {
    return undefined;
  }
}

/** Keeps `value` under `name`, done once it is written (so a change is kept before it is first sent). */
export async function keep(name: string, value: unknown): Promise<void> {
  const d = await db();
  if (d) {
    const ok = await new Promise<boolean>((done) => {
      try {
        const t = d.transaction(STORE, 'readwrite');
        t.objectStore(STORE).put(value, name);
        t.oncomplete = () => done(true);
        t.onerror = () => done(false);
        t.onabort = () => done(false);
      } catch {
        done(false);
      }
    });
    if (ok) {
      try {
        localStorage.removeItem(LS(name));
      } catch {
        /* nothing kept there */
      }
      return;
    }
  }
  try {
    localStorage.setItem(LS(name), JSON.stringify(value));
  } catch {
    /* kept while the page is open */
  }
}
