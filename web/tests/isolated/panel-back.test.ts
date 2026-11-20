// BACK (the owner, 2026-11-20: "going back from a panel to a page, it would show the panel open transition in reverse"): Back walks
// the panel's own trail, closes the panel with its motion reversed, and only then moves the page, on a tablet as on a phone
// (lib/panel's guard, beside the page too); a panel's coming and going slides from its right edge (lib/panelMotion, App); page to
// page stays instant. The real lib/panel on tests/harness/pageGlobals, with a history that keeps its entries and its listeners
// caught as they register; App, which takes a whole app, read. Red first: at 7da786c91 a tablet's Back moved the page at once,
// past its side panel, and nothing said how the panel went.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

const g = globalThis as Record<string, unknown>;
type Entry = { state: unknown; url: string };
const entries: Entry[] = [{ state: null, url: 'http://cratarr.test/#/home' }];
let at = 0;
const heard: Record<string, (() => void)[]> = {};
const fire = (type: string) => (heard[type] ?? []).forEach((f) => f());
const go = (url?: string) => {
  const href = url ? new URL(url, entries[at].url).href : entries[at].url;
  g.location = new URL(href);
  return href;
};
go();
g.history = {
  get state() {
    return entries[at].state;
  },
  replaceState(state: unknown, _title: string, url?: string) {
    entries[at] = { state: structuredClone(state), url: go(url) };
  },
  pushState(state: unknown, _title: string, url?: string) {
    entries.splice(at + 1);
    entries.push({ state: structuredClone(state), url: go(url) });
    at += 1;
  },
  back() {
    at -= 1;
    go();
    fire('popstate');
  },
};
g.addEventListener = (type: string, f: () => void) => void (heard[type] ??= []).push(f);

const { closePlaylist, panel, panelMoved, setBeside, upToPage, viewOver, viewPlaylist } = await import('../../src/lib/panel.svelte.ts');
const { pageHere } = await import('../../src/lib/addresses.ts');
const { PANEL_MS, panelMs } = await import('../../src/lib/panelMotion.ts');
const history = g.history as { back(): void };

/** A link followed to another page (a step of the page's history of its own, no panel's word in it). */
function visit(hash: string) {
  entries.splice(at + 1);
  entries.push({ state: null, url: new URL(hash, entries[at].url).href });
  at += 1;
  go();
  fire('hashchange');
}
const app = readFileSync(new URL('../../src/App.svelte', import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('tablet: Back with a two-step panel trail steps, then closes the panel, and only then moves the page', () => {
  setBeside(true);
  closePlaylist();
  visit('#/library');
  viewPlaylist('album:cratarr/al-1');
  viewOver('artist:cratarr/ar-2');
  assert.equal(panel.viewed, 'artist:cratarr/ar-2');
  history.back();
  assert.equal(panel.viewed, 'album:cratarr/al-1', "Back walks the side panel's own trail first");
  assert.equal(pageHere(), 'library');
  history.back();
  assert.equal(panel.viewed, null, 'then closes it');
  assert.equal(panelMoved().how, 'close', 'with its motion reversed');
  assert.equal(panelMs(panelMoved(), 'close', Date.now(), false), PANEL_MS);
  assert.equal(pageHere(), 'library', 'the page not moved yet');
  history.back();
  assert.equal(pageHere(), 'home', 'only then the page');
  assert.equal(panel.viewed, null);
});

test("a split page's own part is the page's: Back walks what was opened over it, then moves the page", () => {
  setBeside(true, true);
  closePlaylist();
  visit('#/settings');
  viewPlaylist('settings:account');
  viewOver('settings:account/spotify');
  history.back();
  assert.equal(panel.viewed, 'settings:account', 'the thing opened over the part, walked back');
  history.back();
  assert.equal(pageHere(), 'home', 'the part stays the page\'s: Back moves the page');
  setBeside(true, false);
});

test("phone: the panel's exit runs on Back, over what it showed; its entrance waits for Hold", () => {
  setBeside(false);
  closePlaylist();
  visit('#/crates');
  viewPlaylist('crate:cr-1');
  assert.equal(panelMoved().how, 'open', 'its open, said for its entrance');
  assert.equal(panelMs(panelMoved(), 'open', Date.now(), false), PANEL_MS);
  history.back();
  assert.equal(panel.viewed, null);
  assert.equal(panelMoved().how, 'close');
  assert.equal(panelMs(panelMoved(), 'close', Date.now(), false), PANEL_MS, 'the exit runs');
  // The layer: no fade that ran while Hold kept it hidden; out of sight at its right edge until Hold shows it, then in; out on
  // Back the same way, over its content as it was (an outro keeps it).
  assert.ok(app.includes("{@attach arriving} out:panelSlide={{ duration: panelMsNow('close') }}>"));
  assert.ok(!/class="playlist-layer" class:under-bar[^>]*transition:fade/.test(app), 'its fade gone');
  assert.ok(app.includes('onshown={() => (layerComes(), playlistLayer && letGoHeld())}'));
  assert.match(app, /el\.classList\.remove\('arriving'\);\n    el\.animate\(SLIDE_IN, \{ duration: PANEL_MS, easing: DECELERATE_CSS \}\);/);
  // The tablet's: in and out of its place, lifted out of the page's grid as it goes, so the page widens at once.
  assert.ok(app.includes("in:asideIn={{ duration: panelMsNow('open') }} out:asideOut={{ duration: panelMsNow('close') }}"));
  assert.ok(app.includes("Object.assign(node.style, { position: 'fixed', right: '0', width: `${sideWidth}px`, zIndex: '20' });"));
});

test('page to page: no transition; nor a breadcrumb\'s page, a hand-over, reduced motion, or a word gone stale', () => {
  setBeside(true);
  closePlaylist();
  visit('#/library');
  viewPlaylist('album:cratarr/al-1');
  visit('#/home');
  assert.equal(panel.viewed, null, 'the page gone to closed the panel');
  assert.equal(panelMs(panelMoved(), 'close', Date.now(), false), 0, 'no motion: a page gone to');
  viewPlaylist('album:cratarr/al-1');
  upToPage('library');
  assert.equal(panelMs(panelMoved(), 'close', Date.now(), false), 0, "a breadcrumb's page");
  const now = 1_000_000;
  assert.equal(panelMs({ how: 'close', at: now }, 'close', now + 10, true), 0, 'motion reduced, or a hand-over');
  assert.equal(panelMs({ how: 'close', at: now }, 'close', now + 5_000, false), 0, 'a word gone stale');
  assert.equal(panelMs({ how: 'open', at: now }, 'close', now + 10, false), 0);
  // The page itself takes no transition.
  const page = app.slice(app.indexOf('<div class="page"'), app.indexOf('>', app.indexOf('<div class="page"')));
  assert.doesNotMatch(page, /transition:|in:|out:/);
});
