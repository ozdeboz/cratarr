// Picture check (decision 0057, step 3): a recording of the visuals' frames replayed through two pipelines (today's
// renderer and what replaces it), frame by frame, each frame's screen compared (picture-lab.html). A refactor must
// give the same picture: 0 difference; a merged pass at most 1/255 with a PSNR of 50 dB or more.
//
//   node scripts/check-picture.mjs [names|all] [--a current] [--b current] [--bits 32,16] [--base url]
//   node scripts/check-picture.mjs record <scene>… [--frames 240] [--base url]   into tests/pictures/<scene>.json.gz
//
// It drives a Chromium browser (BROWSER; Edge on Windows by default) on SwiftShader, the same GPU everywhere, over
// its debugging port (CDP_PORT, else one at random; PROFILE: its profile, else a new one). base: the running dev
// server, http://localhost:5173 by default. Exits 1 when a replay differs by more than --tolerance (of 255; 0 by
// default).
import { spawn } from 'node:child_process';
import { mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return fallback;
  const [, value] = args.splice(i, 2);
  return value;
};
const base = option('base', 'http://localhost:5173').replace(/\/$/, '');
const frames = Number(option('frames', 240));
const a = option('a', 'current');
const b = option('b', 'current');
const bits = option('bits', '32,16').split(',').map(Number);
const tolerance = Number(option('tolerance', 0));
const recording = args[0] === 'record';
if (recording) args.shift();
const pictures = join(dirname(fileURLToPath(import.meta.url)), '..', 'tests', 'pictures');
const names = !args.length || args[0] === 'all' ? readdirSync(pictures).filter((f) => f.endsWith('.json.gz')).map((f) => f.slice(0, -8)) : args;

const browser =
  process.env.BROWSER ?? (process.platform === 'win32' ? 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' : 'chromium');
const port = Number(process.env.CDP_PORT ?? 9300 + Math.floor(Math.random() * 600));
const profile = process.env.PROFILE ?? mkdtempSync(join(tmpdir(), 'picture-'));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const proc = spawn(browser, ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, '--use-angle=swiftshader', '--enable-unsafe-swiftshader', 'about:blank']);
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
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) waiting.get(m.id)(m);
});
const js = async (expression) => (await send('Runtime.evaluate', { expression, returnByValue: true })).result.result?.value;

await send('Page.enable');
await send('Runtime.enable');
// A small screen: SwiftShader draws on the CPU.
await send('Emulation.setDeviceMetricsOverride', { width: 640, height: 360, deviceScaleFactor: 1, mobile: false });

/** Opens the lab at `query` and waits until `ready` (an expression on window.pictureLab) holds, or it fails. */
async function lab(query, ready, minutes = 20) {
  await send('Page.navigate', { url: `${base}/picture-lab.html?${query}` });
  const until = Date.now() + minutes * 60_000;
  while (Date.now() < until) {
    await sleep(1000);
    const state = await js(`(() => { const l = window.pictureLab; return l && { error: l.error, ready: !!(${ready}) }; })()`);
    if (state?.error) throw new Error(state.error);
    if (state?.ready) return;
  }
  throw new Error('timed out');
}

let bad = 0;
try {
  if (recording) {
    for (const scene of names) {
      await lab(`record=${scene}&frames=${frames}`, 'l.request?.done');
      const text = await js('JSON.stringify(window.pictureLab.request.recording)');
      const gz = gzipSync(text, { level: 9 });
      writeFileSync(join(pictures, `${scene}.json.gz`), gz);
      console.log(`recorded ${scene}: ${JSON.parse(text).frames.length} frames, ${(gz.length / 1024).toFixed(0)} KB`);
    }
  } else {
    for (const name of names)
      for (const precision of bits) {
        await lab(`replay=${name}&a=${a}&b=${b}&bits=${precision}`, 'l.result');
        const r = await js('window.pictureLab.result');
        const ok = r.max <= tolerance;
        if (!ok) bad++;
        const psnr = Number.isFinite(r.psnr) ? `${r.psnr.toFixed(1)} dB` : '∞';
        console.log(
          `${ok ? '✓' : '✗'} ${name} at ${precision} (${r.precision.join('/')}): ${a} vs ${b}, ${r.frames} frames: max ${r.max}/255` +
            (r.max ? ` (frame ${r.worstFrame}, ${r.framesDiffering} frames differ)` : '') +
            `, mean ${r.mean.toFixed(4)}, PSNR ${psnr}; lit ${r.lit.toFixed(1)}; tiles ${r.tiles.matched} matched, max ${r.tiles.max.toFixed(4)}; meters max ${r.meters?.toFixed(6) ?? '-'}; passes ${r.passes.map((x) => x.toFixed(1)).join('/')}; digests ${r.digests.join(' ')}; ${r.seconds.toFixed(0)} s`,
        );
      }
  }
} catch (e) {
  console.error(`✗ ${e.message}`);
  bad++;
}
ws.close();
proc.kill();
process.exit(bad ? 1 : 0);
