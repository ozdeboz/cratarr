// BACK's second part (the owner, 2026-11-20: "sometimes using the phone back feature, it closes the app instead of following the back
// button that is clearly present"): the device's Back is the page's own, as it shows. The app asks (`{type: 'back'}`); the page runs a
// sheet's close first, then what is a step over it and its history, then Home; only Home at the visit's first step has none, and the
// app then goes to the background. The rule runs (lib/deviceBack); App, which takes a whole app, is read. Red first: none of it was
// there at 7da786c91, where the app went by the WebView's history alone. The tablet's panel two deep: isolated/panel-back.test.ts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { backDoes, overThePage } from '../src/lib/deviceBack.ts';

const src = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const at = (page: string, navAt = 0, o: Partial<Parameters<typeof backDoes>[0]> = {}) => backDoes({ sheet: false, over: false, navAt, searchAll: false, page, ...o });

test("a cold start on #/settings: the device's Back goes Home, and then has none, so the app goes to the background", () => {
  assert.equal(at('settings'), 'home', 'Back = Home, as the page shows it');
  // Home takes the first step's place, so it is the visit's first step itself.
  assert.equal(at('home'), 'none');
  const app = src('App.svelte');
  assert.match(app, /function backToHome\(\) \{\n    homeInPlace = true;\n    location\.replace\(keepViewed\(HOME\.id\)\);/);
  assert.match(app, /if \(homeInPlace\) \{\n      homeInPlace = false;\n      history\.replaceState\(\{ \.\.\.st, nav: 0 \}, ''\);/, 'Home there is the first step: no Back of its own');
  assert.ok(app.includes('back={pageBack}'), 'the Back it shows is the one the device runs');
  assert.ok(app.includes("if (inApp) onApp('back', (m) => toApp({ type: 'back', id: m.id, done: deviceBack() }));"));
});

test("the page's history first, then Home; Show all of a search is a step too", () => {
  assert.equal(at('library', 2), 'history');
  assert.equal(at('home', 1), 'history', 'Home a step on: back to the page before');
  assert.equal(at('home', 0, { searchAll: true }), 'history');
});

test('what covers the page goes first: the panel\'s trail, the player over everything, the visuals', () => {
  assert.equal(at('home', 0, { over: true }), 'history');
  assert.equal(overThePage({ panelGuard: true, nav: 0 }), true);
  assert.equal(overThePage({ player: true }), true);
  assert.equal(overThePage({ visuals: true, rack: true }), true);
  assert.equal(overThePage({ nav: 3, scroll: 120 }), false);
  assert.equal(overThePage(null), false);
});

test('an open sheet: closed first, as the page closes it (Esc, lib/MenuSheet); its history Back never did', () => {
  assert.equal(at('library', 2, { sheet: true }), 'sheet');
  assert.equal(at('home', 0, { sheet: true }), 'sheet', 'never the background with a sheet open');
  const app = src('App.svelte');
  assert.ok(app.includes(`sheet: !!document.querySelector('.sheet[aria-modal="true"]'),`));
  assert.ok(app.includes("if (does === 'sheet') document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));"));
  // The sheet's own Esc: its step's back where it has one, else its close; caught before anything under it hears the key.
  const sheet = src('lib/MenuSheet.svelte');
  assert.match(sheet, /if \(e\.key !== 'Escape'\) return;\n      e\.stopPropagation\(\);\n      if \(onback\) onback\(\);\n      else onclose\(\);/);
  assert.ok(sheet.includes("addEventListener('keydown', key, true);"));
});
