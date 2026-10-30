// Saved visuals read as the app reads them, and a headless browser to drive a lab page with: shared by
// check-migration.mjs (decision 0057). Run with node's strip-types and tests/ts-resolve.mjs.
//
// A file is a device's kept settings (`{"cratarr-visuals": "<json>"}`, as localStorage holds them), the server's
// snapshots (a JSON list of rows with `name` and `settings`, as `sqlite3 -json` gives them from visual_snapshot or
// /api/visuals/snapshots answers), or one settings object.
import { readFileSync, mkdtempSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

globalThis.$state ??= (v) => v;
globalThis.$state.snapshot ??= (v) => JSON.parse(JSON.stringify(v));

const src = '../src/lib/visuals/';
const { DEFAULTS, keptFrom, fluidFrom63, BEHAVIOUR_PINS, BAND_PINS } = await import(`${src}settings.svelte.ts`);
const { emptyExtra } = await import(`${src}patch.ts`);
const { routeKey, SCALE_KEYS } = await import(`${src}routes.ts`);
const { PRESET_KEYS } = await import(`${src}flame.ts`);
// The preset chains are the tests' fixture since the app reaches them no more (0.9.19).
const { CHAINS, EXAMPLES } = await import('../tests/harness/oracle/chains.ts');

export const copy = (v) => JSON.parse(JSON.stringify(v));

/** A device's kept settings (settings.svelte.ts `load`). */
export function fromDevice(kept) {
  const k = { ...kept };
  if (k.keptForm === undefined && typeof k.fluid === 'number') k.fluid = fluidFrom63(k.fluid);
  return { raw: kept, v1: keptFrom(k) };
}

/** A snapshot as a recall applies it (Visuals.svelte `recall`), over the defaults (a device's own routes beyond the
 *  defaults' are turned off by a recall anyway). */
export function fromSnapshot(saved) {
  const settings = { ...saved, loose: 0 };
  if ((settings.v ?? 1) === 1 && typeof settings.fluid === 'number') settings.fluid = fluidFrom63(settings.fluid);
  for (const k of SCALE_KEYS) if (!(k in settings)) settings[k] = 1;
  for (const k of ['tunnel', 'dive']) if (!(k in settings)) settings[k] = 0;
  for (const k of ['fx1', 'fx1Amount', 'fx2', 'fx2Amount', 'fx3', 'fx3Amount', 'fxLoop', 'fxLoopAmount', 'facets', 'lattice']) if (!(k in settings)) settings[k] = 0;
  for (const k of ['fx1K1', 'fx1K2', 'fx2K1', 'fx2K2', 'fx3K1', 'fx3K2', 'fxLoopK1', 'fxLoopK2']) if (!(k in settings)) settings[k] = 0.5;
  if (!('effects' in settings)) settings.effects = 1;
  if (!('flameLayer' in settings)) settings.flameLayer = 1;
  if (!('extra' in settings)) settings.extra = emptyExtra();
  delete settings.v;
  if (settings.routes && typeof settings.routes === 'object') {
    const off = Object.fromEntries(Object.keys(DEFAULTS.routes).map((k) => [k, 0]));
    const theirs = settings.routes;
    const defaults = {};
    if (!(routeKey('track', null, 'look') in theirs) && !('track|-|drift' in theirs)) defaults[routeKey('track', null, 'look')] = -1;
    if (!(routeKey('track', null, 'behaviour') in theirs)) defaults[routeKey('track', null, 'behaviour')] = -1;
    const fluidRoute = (k) => /[|](stir|flow|swirl)$/.test(k);
    if (!Object.keys(theirs).some(fluidRoute)) for (const [k, v] of Object.entries(DEFAULTS.routes)) if (fluidRoute(k)) defaults[k] = v;
    settings.routes = { ...off, ...defaults, ...theirs };
  }
  // A recall pins the look, the behaviour and the bands' times (a track's preset leaves them).
  return { raw: saved, v1: keptFrom({ ...copy(DEFAULTS), ...settings, pins: [...PRESET_KEYS, ...BEHAVIOUR_PINS, ...BAND_PINS] }) };
}

/** A chain as choosing it applies it: its routes whole, its extras whole, its settings over the rest. */
export function fromChain(c) {
  const off = Object.fromEntries(Object.keys(DEFAULTS.routes).map((k) => [k, 0]));
  const raw = { ...copy(DEFAULTS), ...(c.settings ?? {}), routes: { ...off, ...c.routes }, extra: copy(c.extra) };
  return { raw, v1: keptFrom(raw) };
}

/** Every input of `files`, the built-in chains and examples with `chains`, the defaults with `defaults`. */
export function readInputs(files, { chains = false, defaults = false } = {}) {
  const inputs = [];
  for (const file of files) {
    const data = JSON.parse(readFileSync(file, 'utf8'));
    if (Array.isArray(data))
      for (const row of data) {
        const s = typeof row.settings === 'string' ? JSON.parse(row.settings) : row.settings;
        inputs.push({ name: `snapshot "${row.name}"`, kind: 'snapshot', ...fromSnapshot(s) });
      }
    else if (typeof data['cratarr-visuals'] === 'string') inputs.push({ name: `device ${file.split(/[\\/]/).slice(-2).join('/')}`, kind: 'device', ...fromDevice(JSON.parse(data['cratarr-visuals'])) });
    else inputs.push({ name: `settings ${file}`, kind: 'device', ...fromDevice(data) });
  }
  if (defaults) inputs.push({ name: 'DEFAULTS', kind: 'chain', raw: copy(DEFAULTS), v1: keptFrom(copy(DEFAULTS)) });
  if (chains) for (const c of [...CHAINS, ...EXAMPLES]) inputs.push({ name: `chain ${c.id}`, kind: 'chain', ...fromChain(c) });
  return inputs;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** A headless browser on SwiftShader (BROWSER, else Edge on Windows; CDP_PORT; PROFILE), its one page driven over the
 *  DevTools protocol: `send`, `js` (an expression's value), `serve` (answer requests matching a pattern from here),
 *  `open` (a page of `base`, waiting until `ready` holds on `global`, or its `error`). */
export async function openBrowser(base) {
  const exe = process.env.BROWSER ?? (process.platform === 'win32' ? 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' : 'chromium');
  const port = Number(process.env.CDP_PORT ?? 9300 + Math.floor(Math.random() * 600));
  const profile = process.env.PROFILE ?? mkdtempSync(join(tmpdir(), 'visuals-'));
  const proc = spawn(exe, ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, '--use-angle=swiftshader', '--enable-unsafe-swiftshader', 'about:blank']);
  let target;
  for (let i = 0; i < 80 && !target; i++) {
    await sleep(250);
    try {
      target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === 'page');
    } catch {}
  }
  if (!target) throw new Error(`No browser at ${exe}`);
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener('open', r));
  let id = 0;
  const waiting = new Map();
  const handlers = [];
  ws.addEventListener('message', (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) waiting.get(m.id)(m);
    else if (m.method) for (const h of handlers) h(m);
  });
  const send = (method, params = {}) =>
    new Promise((r) => {
      const n = ++id;
      waiting.set(n, r);
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  const js = async (expression) => (await send('Runtime.evaluate', { expression, returnByValue: true })).result.result?.value;
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 640, height: 360, deviceScaleFactor: 1, mobile: false });
  const served = new Map();
  /** Requests whose URL matches `pattern` (a CDP URL pattern) answered from `body(url)` (a Buffer, or none: 404). */
  const serve = async (pattern, body) => {
    served.set(pattern, body);
    await send('Fetch.enable', { patterns: [...served.keys()].map((urlPattern) => ({ urlPattern })) });
  };
  handlers.push((m) => {
    if (m.method !== 'Fetch.requestPaused') return;
    let got = null;
    for (const f of served.values()) got ??= f(m.params.request.url);
    if (got) send('Fetch.fulfillRequest', { requestId: m.params.requestId, responseCode: 200, responseHeaders: [{ name: 'Content-Type', value: 'application/octet-stream' }], body: got.toString('base64') });
    else send('Fetch.fulfillRequest', { requestId: m.params.requestId, responseCode: 404, body: '' });
  });
  const open = async (page, global, ready, { minutes = 20, progress } = {}) => {
    await send('Page.navigate', { url: `${base}/${page}` });
    const until = Date.now() + minutes * 60_000;
    let said = '';
    while (Date.now() < until) {
      await sleep(1000);
      const state = await js(`(() => { const l = window.${global}; return l && { error: l.error, ready: !!(${ready}), progress: l.progress ?? null }; })()`);
      if (state?.error) throw new Error(state.error);
      if (state?.ready) return;
      if (progress && state?.progress && state.progress !== said) progress((said = state.progress));
    }
    throw new Error('timed out');
  };
  return { send, js, serve, open, close: () => (ws.close(), proc.kill()) };
}
