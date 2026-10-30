// The spike's switch (0075, 2026-10-30): the brake's rules from the WebAssembly where this browser keeps
// `cratarr.rulesWasm` = 1, else this page's own (startStop's JS_BRAKE, kept to compare). Bundled into the page's
// script (as base64, lib/wasm/rulesWasm), so the service worker's copy holds it; nothing more is fetched. Its start-up and its cost per
// call are measured here for the tablet (`cratarrRulesWasm()` from the console, over CDP).
import { JS_BRAKE, useBrake } from '../startStop';
import { brakeRulesOf, instantiateRules, type RulesExports } from './brakeRules';
import b64 from './rulesWasm';

const KEY = 'cratarr.rulesWasm';
const on = () => {
  try {
    return localStorage.getItem(KEY) === '1';
  } catch {
    return false;
  }
};

let ready: Promise<{ exports: RulesExports; startMs: number }> | null = null;
function load() {
  ready ??= (async () => {
    const t0 = performance.now();
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const exports = await instantiateRules(bytes);
    return { exports, startMs: performance.now() - t0 };
  })();
  return ready;
}

if (typeof window !== 'undefined' && on()) load().then(({ exports }) => useBrake(brakeRulesOf(exports)), () => {});

/** The spike's numbers: the start-up (decode, compile, instantiate), and a call's cost each way over `n` calls. */
async function measure(n = 100_000) {
  const { exports, startMs } = await load();
  const w = brakeRulesOf(exports);
  const time = (r: typeof JS_BRAKE) => {
    let sum = 0;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) sum += r.travel(16, 250, (i % 400) / 1000, 1) + r.speed(16, 1, 250, i % 400);
    return { nsPerCall: ((performance.now() - t0) * 1e6) / (2 * n), sum };
  };
  return { using: on() ? 'wasm' : 'js', startMs, wasm: time(w).nsPerCall, js: time(JS_BRAKE).nsPerCall };
}
if (typeof window !== 'undefined') (window as unknown as Record<string, unknown>).cratarrRulesWasm = measure;
