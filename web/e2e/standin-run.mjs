// The browser tests on the stand-in (e2e/standin.mjs): the real page under Vite, its real service worker, a real browser, with
// no built server, so no cargo and no docker. What they prove stops at what the stand-in says; scripts/browser-e2e.sh is the
// real server.
//
//   pnpm run e2e:standin [e2e/some.e2e.mjs ...]        (Node 24)
//
// CHROMIUM: a Chromium already on the machine; without it, Playwright's headless shell where it installs one. playwright-core
// downloads none.
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { standIn } from './standin.mjs';

const web = new URL('..', import.meta.url);

const freePort = () =>
  new Promise((done) => {
    const s = createServer().listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => done(port));
    });
  });

/** Playwright's headless shell, where it installs one here. */
function headlessShell() {
  const root = process.env.LOCALAPPDATA ? join(process.env.LOCALAPPDATA, 'ms-playwright') : join(homedir(), '.cache', 'ms-playwright');
  if (!existsSync(root)) return null;
  for (const dir of readdirSync(root).filter((d) => d.startsWith('chromium_headless_shell-')).sort().reverse()) {
    for (const sub of readdirSync(join(root, dir), { withFileTypes: true }).filter((e) => e.isDirectory())) {
      for (const name of ['chrome-headless-shell.exe', 'chrome-headless-shell', 'headless_shell']) {
        const file = join(root, dir, sub.name, name);
        if (existsSync(file)) return file;
      }
    }
  }
  return null;
}

const chromium = process.env.CHROMIUM ?? headlessShell();
if (!chromium) {
  console.error('no Chromium: set CHROMIUM to one on this machine');
  process.exit(1);
}

const folder = mkdtempSync(join(tmpdir(), 'cratarr-e2e-'));
const accounts = join(folder, 'accounts');
const [serverPort, pagePort] = [await freePort(), await freePort()];
const server = standIn(serverPort, accounts);
const vite = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', String(pagePort), '--strictPort'], {
  cwd: web,
  env: { ...process.env, CRATARR_URL: `http://127.0.0.1:${serverPort}` },
  stdio: 'ignore',
});

let code = 1;
try {
  const page = `http://127.0.0.1:${pagePort}`;
  for (let i = 0; ; i++) {
    if (await fetch(`${page}/`).then((r) => r.ok, () => false)) break;
    if (i > 240) throw new Error('the page did not answer');
    await new Promise((r) => setTimeout(r, 250));
  }
  const files = process.argv.slice(2);
  // Force exit: a file's `after` that fails before it closes its browser would otherwise leave the run waiting for ever, holding the lock.
  const tests = spawn(process.execPath, ['--test', '--test-force-exit', '--test-concurrency=1', '--test-timeout=120000', ...(files.length ? files : ['e2e/*.e2e.mjs'])], {
    cwd: web,
    env: { ...process.env, E2E_PAGE: page, E2E_ACCOUNTS: accounts, CHROMIUM: chromium },
    stdio: 'inherit',
  });
  code = await new Promise((done) => tests.on('exit', done));
} finally {
  vite.kill();
  server.closeAllConnections();
  server.close();
  rmSync(folder, { recursive: true, force: true });
}
process.exit(code);
