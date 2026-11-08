// A stand-in for Cratarr, for the browser tests with no built server (`pnpm run e2e:standin`, e2e/standin-run.mjs; the real
// one is scripts/browser-e2e.sh): sign-in and its session as server/src/serve/signin.rs makes them, the 401 of access.rs,
// /api/me, preferences, an account's few reads, a player's connection and reports, kept for a test to read at
// /api/standin/reports, and the library's index, its feed and each account's own, the rules crate's own bytes
// (server/rules/tests/vectors/index), with small covers in their plain form: it stands at rev 5 until a test moves it to 9 by /api/standin/index-moves, which adds one cover.
// Everything else is not here, which the page takes as it
// takes a server without it. The sign-in page's
// own script is read from signin.rs: what it does to the page's stores is the server's line, not a copy of it. A test takes it
// down and up again by /api/standin/down and /up: down, it answers as its proxy would with Cratarr stopped.
import { randomBytes } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';

const COOKIE = 'cratarr-session';
const challenge = `Cookie realm="Cratarr", form-action="/signin", cookie-name="${COOKIE}"`;
const script = readFileSync(new URL('../../server/src/serve/signin.rs', import.meta.url), 'utf8')
  .match(/<script nonce="\{nonce\}">([\s\S]*?)<\/script>/)[1]
  .replaceAll('{{', '{')
  .replaceAll('}}', '}');

/** What the rules crate wrote of its small library, a file of it. */
const vector = (name) => readFileSync(new URL(`../../server/rules/tests/vectors/index/${name}`, import.meta.url));

/** Small covers as the server's `device_covers` writes them. */
const u32 = (n) => {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
};
const i64 = (n) => {
  const b = Buffer.alloc(8);
  b.writeBigInt64LE(BigInt(n));
  return b;
};
/** A cover a browser can show. */
const JPEG = readFileSync(new URL('./cover.jpg', import.meta.url));
const coverFrame = (record) => Buffer.concat([i64(record), u32(JPEG.length), JPEG]);
const coverBundle = (rev, ...records) => Buffer.concat([Buffer.from('CRXV'), u32(1), i64(rev), ...records.map(coverFrame)]);

const escaped = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
const page = (next) =>
  `<!doctype html><title>Sign in</title><form method="post" action="/signin"><input type="hidden" name="next" value="${escaped(next)}"><input type="hidden" name="browser" value=""><input name="username"><input name="password" type="password"><button>Sign in</button></form><script>${script}</script>`;

const body = (req) =>
  new Promise((done) => {
    let s = '';
    req.on('data', (c) => (s += c));
    req.on('end', () => done(s));
  });
const json = (res, status, v, headers = {}) => {
  res.writeHead(status, { 'content-type': 'application/json', ...headers });
  res.end(JSON.stringify(v));
};

/** On `port`, with two invented accounts, `ann` an administrator and `bob`, their passwords in `accounts` (`name password` a
 *  line, as e2e/browser.mjs reads them). Returns the server, for closing. */
export function standIn(port, accounts) {
  const passwords = { ann: randomBytes(9).toString('hex'), bob: randomBytes(9).toString('hex') };
  writeFileSync(accounts, Object.entries(passwords).map(([n, p]) => `${n} ${p}`).join('\n') + '\n', { mode: 0o600 });
  const sessions = new Map();
  const preferences = {};
  const reports = [];
  let down = false;
  /** Where the library's log stands. */
  let indexRev = 5;
  const whoOf = (req) => {
    const token = (req.headers.cookie ?? '').split(';').map((c) => c.trim().split('=')).find(([n]) => n === COOKIE)?.[1];
    return token ? sessions.get(token) : undefined;
  };
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://x');
    const path = url.pathname;
    const me = whoOf(req);
    if (path === '/signin' && req.method === 'GET') {
      // Served to a device with no live session only: one that has one is sent on.
      if (me) {
        res.writeHead(303, { location: url.searchParams.get('next') ?? '/' });
        return res.end();
      }
      res.writeHead(200, { 'content-type': 'text/html', 'referrer-policy': 'same-origin', 'cache-control': 'no-store' });
      return res.end(page(url.searchParams.get('next') ?? '/'));
    }
    if (path === '/signin' && req.method === 'POST') {
      const form = new URLSearchParams(await body(req));
      const name = form.get('username');
      if (passwords[name] !== form.get('password')) {
        res.writeHead(401, { 'content-type': 'text/html', 'referrer-policy': 'same-origin' });
        return res.end(page('/'));
      }
      const token = randomBytes(32).toString('hex');
      sessions.set(token, name);
      res.writeHead(303, { location: form.get('next') ?? '/', 'set-cookie': `${COOKIE}=${token}; Path=/; Max-Age=3600; HttpOnly; SameSite=Lax`, 'referrer-policy': 'same-origin' });
      return res.end();
    }
    if (path === '/signout' && req.method === 'POST') {
      for (const [t, n] of sessions) if (n === me) sessions.delete(t);
      res.writeHead(303, { location: '/signin', 'set-cookie': `${COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax` });
      return res.end();
    }
    if (!path.startsWith('/api/')) return json(res, 404, { error: 'Not here.' });
    if (path === '/api/standin/down' || path === '/api/standin/up') {
      down = path.endsWith('down');
      return json(res, 200, {});
    }
    if (path === '/api/standin/reports') return json(res, 200, reports);
    if (path === '/api/standin/index-moves') {
      indexRev = 9;
      return json(res, 200, {});
    }
    if (down) {
      res.writeHead(502);
      return res.end();
    }
    if (path === '/api/health') return json(res, 200, { status: 'ok' });
    if (!me) return json(res, 401, { error: 'Sign in to Cratarr.', signin: '/signin' }, { 'www-authenticate': challenge });
    if (path === '/api/me') {
      const permissions = { offline: true, playlists: true, share: true, links: true, request: true, auto_approve: true, manage_requests: true };
      return json(res, 200, { name: me, admin: me === 'ann', permissions, sessions: [{ id: 's', current: true }], devices: [] });
    }
    const held = (preferences[me] ??= {});
    if (path === '/api/me/preferences') return json(res, 200, { values: held });
    const key = path.match(/^\/api\/me\/preferences\/(\w+)$/)?.[1];
    if (key && req.method === 'PUT') {
      const { value, changed } = JSON.parse(await body(req));
      if (!held[key] || changed > held[key].changed) held[key] = { value, changed };
      return json(res, 200, held[key]);
    }
    if (path === '/api/tracks/describe' && req.method === 'POST') {
      // The stand-in tells no track by its id: each comes back not found, as Cratarr tells one it no longer has.
      const { tracks } = JSON.parse(await body(req));
      const none = { name: '', artist: null, album: null, album_id: null, duration_secs: null, addable: false, favorite: null, found: false };
      return json(res, 200, { tracks: tracks.map((t) => ({ ...t, ...none })) });
    }
    if (path === '/api/capabilities') return json(res, 200, { capabilities: ['changes'], version: '0.16.0' });
    if (path === '/api/changes') {
      const since = url.searchParams.get('index');
      const index = { rev: indexRev, changed: Number(since) !== indexRev };
      return json(res, 200, { rev: 1, changes: [], more: false, ...(url.searchParams.get('kinds')?.split(',').includes('index') ? { index } : {}) });
    }
    const bytes = (b) => {
      res.writeHead(200, { 'content-type': 'application/octet-stream', 'cache-control': 'no-store' });
      return res.end(b);
    };
    if (path === '/api/index') return bytes(vector(indexRev === 5 ? 'whole.bin' : 'after.bin'));
    if (path === '/api/index/sounds') return bytes(vector(indexRev === 5 ? 'sounds.bin' : 'sounds-after.bin'));
    if (path === '/api/index/covers') return bytes(indexRev === 5 ? coverBundle(5, 10, 11) : coverBundle(9, 10, 11, 13));
    // Each account's: bob's is another's on the same device (decision 0101 point 28).
    if (path === '/api/index/own') return bytes(vector(me === 'bob' ? 'own-bob.bin' : 'own.bin'));
    if (path === '/api/index/covers/changes') {
      if (Number(url.searchParams.get('since')) === indexRev) {
        res.writeHead(204);
        return res.end();
      }
      return bytes(coverBundle(indexRev, 13));
    }
    if (path === '/api/index/changes' || path === '/api/index/sounds/changes') {
      if (Number(url.searchParams.get('since')) === indexRev) {
        res.writeHead(204);
        return res.end();
      }
      return bytes(vector(path.includes('sounds') ? 'sound-changes.bin' : 'changes.bin'));
    }
    if (path === '/api/albums') return json(res, 200, { whose: me, albums: [], sources: [], trash_days: 14, music: null });
    if (path === '/api/playlists') return json(res, 200, []);
    if (path === '/api/players') return json(res, 200, { players: [] });
    const player = path.match(/^\/api\/players\/[^/]+\/(connect|state)$/)?.[1];
    if (player === 'connect') {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
      return void res.write(': hello\n\n');
    }
    if (player === 'state') {
      reports.push(JSON.parse(await body(req)));
      return json(res, 200, {});
    }
    if (path === '/api/events') {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
      return void res.write(': hello\n\n');
    }
    return json(res, 404, { error: 'Not here.' });
  });
  server.listen(port, '127.0.0.1');
  return server;
}
