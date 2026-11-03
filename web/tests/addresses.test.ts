// The page's addresses (lib/addresses; shared/addresses.json, the owner 2026-11-02: one address a page, a coherent and
// robust whole): the page's table checked against the one the server's and the app's tests read, read and built the
// same way round, and nothing else in the page writing one by hand.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const shared = JSON.parse(readFileSync(new URL('../../shared/addresses.json', import.meta.url), 'utf8'));
const at = (hash: string) => ((globalThis as { location?: unknown }).location = { hash, origin: 'https://cratarr.example' });
at('');
const r = await import('../src/lib/addresses.ts');

test("the page's table is the shared one", () => {
  assert.deepEqual([...r.PAGES], shared.pages);
  assert.equal(r.HOME, shared.home);
  assert.deepEqual([...r.PANEL_WORDS], shared.panel_words);
  assert.deepEqual(Object.fromEntries(Object.entries(r.PAGE_WORDS).map(([k, v]) => [k, [...(v as string[])]])), shared.page_words);
});

test('every page App shows is in the table, and every page in it is one App shows', () => {
  const app = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8');
  const shown = [...app.matchAll(/href: hrefOf\('([a-z]+)'\)/g)].map((m) => m[1]);
  assert.deepEqual([...shown].sort(), [...r.PAGES].sort());
});

test('an address names its page; none, or one no page has, names none (App writes it as Home)', () => {
  for (const p of r.PAGES) {
    assert.equal(r.pageIn(`#/${p}`), p);
    assert.equal(r.pageIn(`#/${p}?view=album%3Acratarr%2F1`), p);
  }
  for (const h of ['', '#', '#/', '#/?view=x', '#/incoming', '#/collection', '#/library/tracks', '#/local/albums', '#/Home'])
    assert.equal(r.pageIn(h), null, h);
  at('#/nowhere?view=x');
  assert.equal(r.pageHere(), 'home');
});

test('built and read back the same', () => {
  assert.equal(r.addressOf('library'), '#/library');
  assert.equal(r.addressOf('find', { q: 'jazz & blues' }), '#/find?q=jazz+%26+blues');
  assert.equal(r.addressOf('playlists', { view: ['a', 'b'], tab: null }), '#/playlists?view=a&view=b');
  const a = r.viewOn('settings', 'settings:connections/lidarr');
  assert.equal(r.pageIn(a), 'settings');
  assert.equal(r.wordsIn(a).get('view'), 'settings:connections/lidarr');
  at('');
  assert.equal(r.linkTo('crates', 'crate:7'), 'https://cratarr.example/#/crates?view=crate%3A7');
});

test("a page's own words are kept, the panel's are not", () => {
  at('#/library?composer=bach&view=album%3Acratarr%2F1&tab=export');
  assert.equal(r.ownWordsHere().toString(), 'composer=bach');
  assert.equal(r.hereWith((w) => w.delete('view')), '#/library?composer=bach&tab=export');
  at('#/crates?composer=bach&view=crate%3A1');
  assert.equal(r.ownWordsHere().toString(), '', "the Library's composer is no word of Crates'");
});

test("the links the server and the app make lead to a page and a Settings part that are there", () => {
  const sections = readFileSync(new URL('../src/lib/settingsSections.svelte.ts', import.meta.url), 'utf8');
  for (const [name, link] of Object.entries(shared.links) as [string, { page: string; view?: string; word?: string }][]) {
    assert.ok(r.isPage(link.page), name);
    if (link.word) assert.ok(r.PAGE_WORDS[link.page as 'settings']?.includes(link.word), `${name}: ${link.word} is the page's own`);
    const part = link.view?.match(/^settings:([a-z]+)/)?.[1];
    if (part) assert.match(sections, new RegExp(`id: '${part}'`), `${name}: Settings has ${part}`);
  }
});

test('nothing but lib/addresses writes or reads an address by hand', () => {
  const src = new URL('../src/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
  const files = (d: string): string[] => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? files(join(d, f)) : /\.(ts|svelte)$/.test(f) ? [join(d, f)] : []));
  const by: string[] = [];
  for (const f of files(src)) {
    if (f.replace(/\\/g, '/').endsWith('lib/addresses.ts')) continue;
    readFileSync(f, 'utf8')
      // Either line ending (a checkout on Windows has \r\n: the \r left on a line kept its comment from being seen).
      .split(/\r?\n/)
      .forEach((line, i) => {
        const code = line.replace(/\/\/.*$/, '').replace(/^\s*\*.*$/, '');
        if (/['"`]#\/|hash\.split\(/.test(code)) by.push(`${f.slice(src.length)}:${i + 1}: ${line.trim()}`);
      });
  }
  assert.deepEqual(by, []);
});
