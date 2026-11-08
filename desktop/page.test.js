// page.js: which page answers, and how (`npm test`).
'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { buildOf, compare, ownPage, prefer } = require('./page');

const ORIGIN = 'https://cratarr.example.org';
const get = (p, method = 'GET') => new Request(ORIGIN + p, { method });

/** A page of release 0.15.0, build AAA, in a folder of its own. */
function aPage() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cratarr-page-'));
  const put = (rel, text) => {
    fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
    fs.writeFileSync(path.join(dir, rel), text);
  };
  put('index.html', '<p>the app’s page</p>');
  put('build.json', JSON.stringify({ version: '0.15.0', build: 'AAA' }));
  put('assets/index-AAA.js', 'app();');
  put('sw.js', 'sw();');
  put('icons/icon-192.png', 'png');
  return dir;
}

/** Cratarr as the page's requests find it: answering `answer`, counted. */
function cratarr(answer = () => new Response('Cratarr’s', { status: 200 })) {
  const f = async (req, signal) => {
    f.asked.push(new URL(req.url).pathname);
    return answer(req, signal);
  };
  f.asked = [];
  return f;
}

/** Cratarr behind a network gone silent: no answer ever, the request let go only when told (counted in `letGo`). */
function silent() {
  const f = cratarr(
    (_req, signal) =>
      new Promise((_, reject) =>
        signal?.addEventListener('abort', () => {
          f.letGo++;
          reject(signal.reason);
        }),
      ),
  );
  f.letGo = 0;
  return f;
}

const newer = () => ({ version: '0.16.0', build: 'ZZZ' });
const sameBuiltAgain = () => ({ version: '0.15.0', build: 'BBB' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

test('releases in order', () => {
  assert.equal(compare('0.15.0', '0.16.0'), -1);
  assert.equal(compare('0.16.0', '0.15.9'), 1);
  assert.equal(compare('0.15', '0.15.0'), 0);
  assert.equal(compare('0.15.x', '0.15.0'), 0, 'a part that is not a number counts as 0');
});

test('the app’s page or Cratarr’s, by release and build', () => {
  const mine = { version: '0.15.0', build: 'AAA' };
  assert.equal(prefer(mine, { version: '0.14.2', build: 'ZZZ' }), true, 'Cratarr older: the app’s');
  assert.equal(prefer(mine, { version: '0.16.0', build: 'ZZZ' }), false, 'Cratarr newer: Cratarr’s');
  assert.equal(prefer(mine, { version: '0.15.0', build: 'AAA' }), true, 'the same build: the app’s');
  assert.equal(prefer(mine, { version: '0.15.0', build: 'BBB' }), false, 'the same release built again: Cratarr’s');
});

test('a build says its release and its build, or is none', () => {
  assert.deepEqual(buildOf({ version: '0.15.0', build: 'AAA' }), { version: '0.15.0', build: 'AAA' });
  assert.equal(buildOf({ version: '0.15.0', build: null }), null);
  assert.equal(buildOf({ version: '', build: 'AAA' }), null);
  assert.equal(buildOf(null), null);
  assert.equal(buildOf('0.15.0'), null);
});

test('the page’s files answer as the server would, Cratarr’s own addresses never', () => {
  const page = ownPage(aPage(), () => null);
  assert.deepEqual(page.own, { version: '0.15.0', build: 'AAA' });
  assert.equal(page.fileFor('/'), 'index.html');
  assert.equal(page.fileFor('/albums/abc'), 'index.html', 'the page’s own routes');
  assert.equal(page.fileFor('/assets/index-AAA.js'), 'assets/index-AAA.js');
  assert.equal(page.fileFor('/assets/index-BBB.js'), null, 'a file the app lacks: Cratarr’s');
  assert.equal(page.fileFor('/guest.html'), null);
  for (const p of ['/api', '/api/health', '/rest/ping.view', '/signin', '/signout', '/guest', '/__device/cover/1']) assert.equal(page.fileFor(p), null, p);
});

test('Cratarr not heard from, or older, or the same build: the app’s page, Cratarr not asked', async () => {
  for (const heard of [null, { version: '0.14.0', build: 'ZZZ' }, { version: '0.15.0', build: 'AAA' }]) {
    const c = cratarr();
    const page = ownPage(aPage(), () => heard);
    const r = await page.answer(get('/library'), c);
    assert.equal(await r.text(), '<p>the app’s page</p>');
    assert.match(r.headers.get('content-type'), /^text\/html/);
    assert.equal(r.headers.get('cache-control'), 'no-cache');
    const js = await page.answer(get('/assets/index-AAA.js'), c);
    assert.match(js.headers.get('content-type'), /^text\/javascript/);
    assert.equal(js.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.deepEqual(c.asked, []);
  }
});

test('Cratarr newer, or the same release built again: Cratarr’s page; Vite’s files still the app’s', async () => {
  for (const heard of [{ version: '0.16.0', build: 'ZZZ' }, { version: '0.15.0', build: 'BBB' }]) {
    const c = cratarr();
    const page = ownPage(aPage(), () => heard);
    assert.equal(await (await page.answer(get('/'), c)).text(), 'Cratarr’s');
    assert.equal(await (await page.answer(get('/sw.js'), c)).text(), 'Cratarr’s');
    assert.equal(await (await page.answer(get('/assets/index-AAA.js'), c)).text(), 'app();', 'the same name, the same bytes');
    assert.deepEqual(c.asked, ['/', '/sw.js']);
  }
});

test('Cratarr’s page chosen but out of reach or failing: the app’s page opens; a redirect is Cratarr’s', async () => {
  const away = cratarr(() => {
    throw new TypeError('net::ERR_INTERNET_DISCONNECTED');
  });
  assert.equal(await (await ownPage(aPage(), newer).answer(get('/'), away)).text(), '<p>the app’s page</p>');
  const failing = cratarr(() => new Response('Bad Gateway', { status: 502 }));
  assert.equal(await (await ownPage(aPage(), newer).answer(get('/library'), failing)).text(), '<p>the app’s page</p>');
  const broken = cratarr(() => new Response('Internal Server Error', { status: 500 }));
  assert.equal(await (await ownPage(aPage(), newer).answer(get('/'), broken)).text(), '<p>the app’s page</p>', 'an error page in its place');
  const signIn = cratarr(() => new Response(null, { status: 303, headers: { location: '/signin?next=%2F' } }));
  const r = await ownPage(aPage(), newer).answer(get('/'), signIn);
  assert.equal(r.status, 303);
  assert.equal(r.headers.get('location'), '/signin?next=%2F');
  // The service worker's script is Cratarr’s or none: the browser keeps the worker it has.
  await assert.rejects(ownPage(aPage(), newer).answer(get('/sw.js'), away));
  // Another file Cratarr answers is Cratarr’s answer, an error too (404: its page has no such file).
  const gone = cratarr(() => new Response('Not Found', { status: 404 }));
  assert.equal((await ownPage(aPage(), newer).answer(get('/build.json'), gone)).status, 404);
});

test('Cratarr silent for its page: the app’s page within the wait, the request let go', async () => {
  for (const heard of [newer, sameBuiltAgain]) {
    const page = ownPage(aPage(), heard, { waitMs: 50 });
    const c = silent();
    const started = Date.now();
    assert.equal(await (await page.answer(get('/'), c)).text(), '<p>the app’s page</p>');
    assert.ok(Date.now() - started < 1_000, `within the wait, not ${Date.now() - started} ms`);
    assert.equal(c.letGo, 1, 'the request let go');
    assert.equal(page.away(), true);
  }
  // A way out that does not listen to being let go: the wait holds all the same.
  const deaf = cratarr(() => new Promise(() => {}));
  assert.equal(await (await ownPage(aPage(), newer, { waitMs: 50 }).answer(get('/'), deaf)).text(), '<p>the app’s page</p>');
});

test('Cratarr out of reach: the page’s files are the app’s at once, the service worker’s script none', async () => {
  const page = ownPage(aPage(), newer, { waitMs: 50 });
  await page.answer(get('/'), silent());
  const c = silent();
  const started = Date.now();
  assert.equal(await (await page.answer(get('/library'), c)).text(), '<p>the app’s page</p>');
  assert.deepEqual(await (await page.answer(get('/build.json'), c)).json(), { version: '0.15.0', build: 'AAA' });
  assert.equal(await (await page.answer(get('/icons/icon-192.png'), c)).text(), 'png');
  await assert.rejects(page.answer(get('/sw.js'), c));
  assert.ok(Date.now() - started < 40, 'nothing waited for');
  assert.deepEqual(c.asked, [], 'Cratarr not asked');
  // A file whose wait runs out on its own (Cratarr not yet out of reach) is the app’s too, the worker’s script not.
  assert.deepEqual(await (await ownPage(aPage(), newer, { waitMs: 50 }).answer(get('/build.json'), silent())).json(), { version: '0.15.0', build: 'AAA' });
  await assert.rejects(ownPage(aPage(), newer, { waitMs: 50 }).answer(get('/sw.js'), silent()));
});

test('Out of reach ends when Cratarr answers anything, or after its time', async () => {
  const page = ownPage(aPage(), newer, { waitMs: 50 });
  await page.answer(get('/'), silent());
  // The page's own request answered (its API): Cratarr is back, and its page is asked for again.
  await page.answer(get('/api/health'), cratarr());
  assert.equal(page.away(), false);
  const c = cratarr();
  assert.equal(await (await page.answer(get('/'), c)).text(), 'Cratarr’s');
  assert.deepEqual(c.asked, ['/']);
  // A proxy's refusal in front of it is no answer: out of reach again; the health check's answer ends it too.
  await page.answer(get('/api/health'), cratarr(() => new Response(null, { status: 502 })));
  assert.equal(page.away(), false, 'a request of the page’s that is refused does not make Cratarr out of reach');
  await page.answer(get('/'), cratarr(() => new Response(null, { status: 403 })));
  assert.equal(page.away(), true, 'the page refused by the proxy does');
  page.reached();
  assert.equal(page.away(), false);
  // After its time, Cratarr is asked again.
  const short = ownPage(aPage(), newer, { waitMs: 50, awayMs: 30 });
  await short.answer(get('/'), silent());
  await sleep(40);
  const again = cratarr();
  assert.equal(await (await short.answer(get('/'), again)).text(), 'Cratarr’s');
  assert.deepEqual(again.asked, ['/']);
});

test('what is not a page file goes to Cratarr as it came', async () => {
  const c = cratarr();
  const page = ownPage(aPage(), () => null);
  await page.answer(get('/', 'POST'), c);
  await page.answer(get('/api/health'), c);
  await page.answer(get('/signin'), c);
  assert.deepEqual(c.asked, ['/', '/api/health', '/signin']);
});

test('an app without a page of its own: Cratarr’s, always', async () => {
  const c = cratarr();
  const page = ownPage(path.join(os.tmpdir(), 'cratarr-no-page-here'), () => null);
  assert.equal(page.own, null);
  assert.equal(page.serves(), false);
  assert.equal(await (await page.answer(get('/'), c)).text(), 'Cratarr’s');
  assert.deepEqual(c.asked, ['/']);
});
