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

/** `refused`: how many opens to come fail, as one does on a transient error or behind another tab's connection. */
export const opening = { refused: 0 };
/** The connections open, by database: another tab's upgrade asks each to close (`upgradeElsewhere`). */
const connections = new Map<string, Set<{ onversionchange?: (() => void) | null }>>();
/** Another tab opens `name` at a newer version: each connection here is told, as a browser tells it. */
export const upgradeElsewhere = (name: string) => [...(connections.get(name) ?? [])].forEach((c) => c.onversionchange?.());
/** How many connections to `name` are open. */
export const openConnections = (name: string) => connections.get(name)?.size ?? 0;

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

function database(stores: Map<string, Store>, name: string) {
  let closed = false;
  const connection = {
    onversionchange: null as (() => void) | null,
    get closed() {
      return closed;
    },
    objectStoreNames: { contains: (n: string) => stores.has(n) },
    createObjectStore: (n: string, o?: { keyPath?: string }) => void stores.set(n, { keyPath: o?.keyPath, rows: new Map() }),
    close() {
      closed = true;
      connections.get(name)?.delete(connection);
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
  connections.set(name, (connections.get(name) ?? new Set()).add(connection));
  return connection;
}

export const fakeIndexedDB = {
  databases: async () => [...databases.keys()].map((name) => ({ name })),
  open(name: string) {
    const r: Handlers & { onupgradeneeded?: (() => void) | null; onblocked?: (() => void) | null } = {};
    later(() => {
      if (opening.refused > 0) {
        opening.refused--;
        r.error = new DOMException('The open was refused.', 'UnknownError');
        return r.onerror?.();
      }
      const fresh = !databases.has(name);
      const stores = databases.get(name) ?? new Map<string, Store>();
      databases.set(name, stores);
      r.result = database(stores, name);
      if (fresh) r.onupgradeneeded?.();
      r.onsuccess?.();
    });
    return r;
  },
};

(globalThis as Record<string, unknown>).indexedDB = fakeIndexedDB;
