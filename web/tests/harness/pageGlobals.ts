// What the page's modules find around them in a browser, stood in for, so that the real ones load in a test (tests/isolated:
// a process of its own, as this fills in the globals). Import it before any of them. `window` stays out, which keeps what
// only a browser's page starts (its listeners, its pollers) from starting. Svelte's runes do nothing here: `$state` is its
// value, and a test runs an `$effect` by hand from `effects`.
import { registerHooks, stripTypeScriptTypes } from 'node:module';

// lib/api's ApiError declares its fields in its constructor, which Node's stripping of types refuses. And what Vite makes an
// address of (`?url`, a worker's and a wasm file's) is only its address here: nothing fetches it, and neither would load.
registerHooks({
  load(url, context, next) {
    if (/[?&]url$/.test(url)) return { format: 'module', source: `export default ${JSON.stringify(url)};`, shortCircuit: true };
    const r = next(url, context);
    if (!url.endsWith('/lib/api.ts')) return r;
    return { ...r, format: 'module', source: stripTypeScriptTypes(new TextDecoder().decode(r.source as Uint8Array), { mode: 'transform' }) };
  },
});

/** Every `$effect` the modules made, in order. */
export const effects: (() => void)[] = [];

const memory = new Map<string, string>();
const g = globalThis as Record<string, unknown>;
g.localStorage = {
  getItem: (k: string) => memory.get(k) ?? null,
  setItem: (k: string, v: string) => void memory.set(k, v),
  removeItem: (k: string) => void memory.delete(k),
  key: (i: number) => [...memory.keys()][i] ?? null,
  get length() {
    return memory.size;
  },
};
g.$state = Object.assign(<T>(v: T) => v, { snapshot: <T>(v: T) => v, raw: <T>(v: T) => v });
g.$effect = Object.assign((run: () => void) => void effects.push(run), { root: (f: () => void) => (f(), () => {}), pre: () => {}, tracking: () => false });
g.$derived = Object.assign(<T>(v: T) => v, { by: <T>(f: () => T) => f() });
g.document = Object.assign(new EventTarget(), { hidden: false, documentElement: { dataset: {}, removeAttribute() {}, style: { setProperty() {} } } });
g.addEventListener = () => {};
g.location = new URL('http://cratarr.test/');
g.history = { state: null, replaceState() {}, pushState() {} };

// The page's own timers (its pollers) keep no test running.
const later = setTimeout;
g.setTimeout = (...args: Parameters<typeof setTimeout>) => later(...args).unref();
