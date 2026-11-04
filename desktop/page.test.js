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
  const f = async (req) => {
    f.asked.push(new URL(req.url).pathname);
    return answer(req);
  };
  f.asked = [];
  return f;
}

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
  const newer = () => ({ version: '0.16.0', build: 'ZZZ' });
  const page = ownPage(aPage(), newer);
  const away = cratarr(() => {
    throw new TypeError('net::ERR_INTERNET_DISCONNECTED');
  });
  assert.equal(await (await page.answer(get('/'), away)).text(), '<p>the app’s page</p>');
  const failing = cratarr(() => new Response('Bad Gateway', { status: 502 }));
  assert.equal(await (await page.answer(get('/library'), failing)).text(), '<p>the app’s page</p>');
  const signIn = cratarr(() => new Response(null, { status: 303, headers: { location: '/signin?next=%2F' } }));
  const r = await page.answer(get('/'), signIn);
  assert.equal(r.status, 303);
  assert.equal(r.headers.get('location'), '/signin?next=%2F');
  // Only the page itself falls back: another file (the service worker's script) is Cratarr’s or nothing.
  await assert.rejects(page.answer(get('/sw.js'), away));
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
