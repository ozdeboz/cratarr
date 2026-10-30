// Phone check (decided 2026-10-16): every page, every part of the settings, an album and an artist in the panel, at a phone's
// width (390 px, as touch), must not scroll sideways: neither the page nor any box that scrolls up and down. A box
// meant to scroll sideways (a row of chips) scrolls only that way, so it is not looked at. A page that throws, or that
// the dev server cannot build (its error overlay), fails too.
//
//   node scripts/check-phone.mjs [base]        base: the running page, http://localhost:5173 by default
//
// It drives a Chromium browser over its debugging port: BROWSER names it (Edge on Windows by default). Exits 1 when
// something scrolls sideways, saying where.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const base = (process.argv[2] ?? 'http://localhost:5173').replace(/\/$/, '');
const browser =
  process.env.BROWSER ?? (process.platform === 'win32' ? 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' : 'chromium');
const port = 9300 + Math.floor(Math.random() * 600);
const SETTINGS = ['connections', 'libraries', 'players', 'auth', 'import', 'display', 'diagnostics'];
const PAGES = ['library', 'playlists', 'crates', 'insights', 'health', 'settings'];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const proc = spawn(browser, ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), 'phone-'))}`, '--disable-gpu', 'about:blank']);
let target;
for (let i = 0; i < 80 && !target; i++) {
  await sleep(250);
  try {
    target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === 'page');
  } catch {}
}
if (!target) {
  console.error(`No browser at ${browser}`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r));
let id = 0;
const waiting = new Map();
const send = (method, params = {}) =>
  new Promise((r) => {
    const n = ++id;
    waiting.set(n, r);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
/** What the page threw since it was last asked. */
let thrown = [];
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) return waiting.get(m.id)(m);
  if (m.method === 'Runtime.exceptionThrown') thrown.push((m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text).split('\n')[0]);
});
const js = async (expression) => (await send('Runtime.evaluate', { expression, returnByValue: true })).result.result.value;

await send('Page.enable');
await send('Runtime.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await send('Emulation.setTouchEmulationEnabled', { enabled: true });

// What scrolls sideways: the page, and any box that scrolls up and down but is wider inside than it shows.
const SIDEWAYS = `(() => {
  const found = [];
  const doc = document.scrollingElement;
  if (doc.scrollWidth > doc.clientWidth + 1) found.push('the page: ' + doc.scrollWidth + ' > ' + doc.clientWidth);
  for (const el of document.querySelectorAll('*')) {
    const st = getComputedStyle(el);
    if (!/(auto|scroll)/.test(st.overflowY) || el.clientHeight === 0) continue;
    if (el.scrollWidth > el.clientWidth + 1) {
      const name = el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.trim().split(/\\s+/).join('.') : '');
      const wide = [...el.querySelectorAll('*')].filter((k) => k.getBoundingClientRect().right > el.getBoundingClientRect().right + 1).slice(0, 2)
        .map((k) => k.tagName.toLowerCase() + (typeof k.className === 'string' && k.className ? '.' + k.className.trim().split(/\\s+/)[0] : ''));
      found.push(name + ': ' + el.scrollWidth + ' > ' + el.clientWidth + (wide.length ? ' (' + wide.join(', ') + ')' : ''));
    }
  }
  return found;
})()`;

async function visit(hash, settle = 2500) {
  thrown = [];
  await send('Page.navigate', { url: `${base}/#${hash}` });
  await sleep(settle);
  const found = await js(SIDEWAYS);
  if (await js(`!!document.querySelector('vite-error-overlay')`)) found.push('the dev server could not build it (its error overlay)');
  return [...found, ...thrown.map((t) => `threw: ${t}`)];
}

const views = PAGES.map((p) => `/${p}`);
for (const s of SETTINGS) views.push(`/settings?view=settings:${s}`);
try {
  const albums = await (await fetch(`${base}/api/albums`)).json();
  const s = albums.sources?.find((x) => x.albums?.length);
  if (s) views.push(`/library?view=album:${s.id}/${s.albums[0].id}`);
  const by = s?.albums.find((a) => a.artist)?.artist;
  if (by) views.push(`/library?view=artist:${s.id}/${encodeURIComponent(by)}`);
} catch {
  /* no albums to look at */
}

let bad = 0;
for (const v of views) {
  // Settings parts open on the server's answer, which checks every connection: give them longer.
  const found = await visit(v, v.includes('settings:') ? 7000 : 2500);
  if (found.length) {
    bad++;
    console.log(`✗ ${v}\n  ${found.join('\n  ')}`);
  } else {
    console.log(`✓ ${v}`);
  }
}
ws.close();
proc.kill();
process.exit(bad ? 1 : 0);
