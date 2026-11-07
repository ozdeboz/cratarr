// IndexedDB in memory, as much of it as the page's stores use (lib/keptStore, lib/localCovers, lib/shelfStore), so that their
// real code runs in a test and a test can say which database holds what. Requests answer on the next turn, as a
// browser's do. Install it before the page's modules load.

type Store = { keyPath?: string; rows: Map<unknown, unknown> };
type Handlers = { onsuccess?: (() => void) | null; onerror?: (() => void) | null; result?: unknown; error?: unknown };

/** Every database opened, by name: each store's rows. */
export const databases = new Map<string, Map<string, Store>>();

/** A store's rows in a database, as they are; none where it was never made. */
export const rowsOf = (db: string, store: string) => databases.get(db)?.get(store)?.rows ?? new Map<unknown, unknown>();

/** Set, writes are refused as a browser refuses one past its quota: the transaction aborts, and nothing of it lands. `only`: to that
 *  store alone, as a big write can be refused and a small one taken. */
export const storage = { refused: false, only: null as string | null };
const refuses = (store: string | string[]) => storage.refused && (storage.only === null || storage.only === store);

const later = (f: () => void) => void setImmediate(f);

function request<T>(work: () => T): Handlers {
  const r: Handlers = {};
  later(() => {
    r.result = work();
    r.onsuccess?.();
  });
  return r;
}

function objectStore(s: Store, refused: () => boolean) {
  const keyOf = (value: unknown, key?: unknown) => key ?? (value as Record<string, unknown>)[s.keyPath!];
  return {
    put: (value: unknown, key?: unknown) => request(() => void (refused() || s.rows.set(keyOf(value, key), value))),
    delete: (key: unknown) => request(() => void (refused() || s.rows.delete(key))),
    clear: () => request(() => void (refused() || s.rows.clear())),
    get: (key: unknown) => request(() => s.rows.get(key)),
    getAll: () => request(() => [...s.rows.values()]),
    openCursor: () => {
      const r: Handlers = {};
      const entries = [...s.rows.entries()];
      let at = 0;
      const step = () =>
        later(() => {
          const here = entries[at++];
          r.result = here ? { key: here[0], value: here[1], continue: step } : null;
          r.onsuccess?.();
        });
      step();
      return r;
    },
  };
}

function database(stores: Map<string, Store>) {
  let closed = false;
  return {
    objectStoreNames: { contains: (n: string) => stores.has(n) },
    createObjectStore: (n: string, o?: { keyPath?: string }) => void stores.set(n, { keyPath: o?.keyPath, rows: new Map() }),
    close() {
      closed = true;
    },
    transaction(names: string | string[], mode?: string) {
      // As a browser refuses it: a write that outlives its database throws, and is not quietly kept.
      if (closed) throw new DOMException('The database connection is closing.', 'InvalidStateError');
      const t: { oncomplete?: (() => void) | null; onerror?: (() => void) | null; onabort?: (() => void) | null; error: unknown; objectStore: (n: string) => ReturnType<typeof objectStore> } = {
        error: null,
        objectStore: (n: string) => {
          const s = stores.get(n);
          if (!s) throw new Error(`no store ${n} in ${String(names)}`);
          return objectStore(s, () => refuses(names));
        },
      };
      // After the requests made in this turn, and their answers.
      later(() => later(() => (refuses(names) && mode === 'readwrite' ? t.onabort?.() : t.oncomplete?.())));
      return t;
    },
  };
}

export const fakeIndexedDB = {
  databases: async () => [...databases.keys()].map((name) => ({ name })),
  open(name: string) {
    const r: Handlers & { onupgradeneeded?: (() => void) | null; onblocked?: (() => void) | null } = {};
    later(() => {
      const fresh = !databases.has(name);
      const stores = databases.get(name) ?? new Map<string, Store>();
      databases.set(name, stores);
      r.result = database(stores);
      if (fresh) r.onupgradeneeded?.();
      r.onsuccess?.();
    });
    return r;
  },
};

(globalThis as Record<string, unknown>).indexedDB = fakeIndexedDB;
