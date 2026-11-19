// F25 with SB15: a panel step's place rides with it in the panel's trail (lib/panel). The real module on tests/harness/pageGlobals,
// with a history that keeps its entries and the module's popstate and hashchange listeners caught as they register.
import { test } from 'node:test';
import assert from 'node:assert/strict';
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

const { backOne, closePlaylist, keepPlace, placeHere, placeOf, reachedBy, setBeside, viewOver, viewPlaylist, viewSibling } = await import('../../src/lib/panel.svelte.ts');
const history = g.history as { back(): void };

/** The browser taken to another page over the panel (its Home): a new step of the page's history, no state of the panel's. */
function home() {
  entries.splice(at + 1);
  entries.push({ state: null, url: new URL('#/home', entries[at].url).href });
  at += 1;
  go();
  fire('hashchange');
}

test('F25.1: back to a panel step brings its place back; a new step starts at the top', () => {
  setBeside(true);
  closePlaylist();
  viewPlaylist('requests:mine');
  keepPlace({ scroll: 840 });
  viewOver('album:cratarr/al-42');
  keepPlace({ scroll: 120 });
  backOne();
  assert.deepEqual(placeOf(), { scroll: 840 }, "Yours' place, not the record's");
  assert.equal(reachedBy(), 'back');
  viewOver('album:cratarr/al-42');
  assert.equal(placeOf(), null, 'the record opened again later as a new step: its top');
  assert.equal(reachedBy(), 'forward');
});

test('F25.4: the place is written once more as its step is left, so the last scroll counts', () => {
  setBeside(true);
  closePlaylist();
  viewPlaylist('health:incomplete');
  keepPlace({ scroll: 300, open: 'a/al-7' });
  const here = placeHere();
  here.leaving(() => here.keep({ scroll: 960, open: 'a/al-7' }));
  viewOver('album:cratarr/al-7');
  backOne();
  assert.deepEqual(placeOf(), { scroll: 960, open: 'a/al-7' }, "its last scroll, and Health's open row with it");
});

test('F25.3: a forward step, viewed again, a sibling: their own rules; back from a sibling to its place', () => {
  setBeside(true);
  closePlaylist();
  viewPlaylist('requests:mine');
  keepPlace({ scroll: 840 });
  viewSibling('requests:soulseek');
  assert.equal(placeOf(), null, 'a sibling picked: a step of its own, at its top');
  keepPlace({ scroll: 50 });
  backOne();
  assert.deepEqual(placeOf(), { scroll: 840 }, 'Yours back at its place');
  viewPlaylist('requests:mine');
  assert.equal(reachedBy(), 'back', 'viewed again where it shows: no new step (its list scrolls to the top by panel.again)');
});

test('F25.6 (c), (d): back twice quickly restores only the step shown at the end; a new step during the tries starts at its top', () => {
  setBeside(true);
  closePlaylist();
  viewPlaylist('artist:cratarr/Kite Theory');
  keepPlace({ scroll: 2400 });
  viewOver('album:cratarr/al-1');
  keepPlace({ scroll: 700 });
  viewOver('label:la-3');
  backOne();
  backOne();
  assert.deepEqual(placeOf(), { scroll: 2400 }, "the artist's, never the record's");
  viewOver('album:cratarr/al-2');
  assert.equal(placeOf(), null);
});

test('F25.5 (b): on a phone, the browser Back over the guard shows the step before at its kept place', () => {
  setBeside(true);
  closePlaylist();
  setBeside(false);
  viewPlaylist('requests:mine');
  keepPlace({ scroll: 840 });
  viewOver('album:cratarr/al-42');
  keepPlace({ scroll: 120 });
  assert.ok((g.history as { state: { panelGuard?: boolean } }).state?.panelGuard, 'one guard stands for the trail');
  history.back();
  assert.deepEqual(placeOf(), { scroll: 840 });
  assert.equal(reachedBy(), 'back');
});

test("F25.5 (c): on a phone, the page's Home over the panel, then Back: the trail comes back with its places", () => {
  setBeside(true);
  closePlaylist();
  setBeside(false);
  viewPlaylist('requests:mine');
  keepPlace({ scroll: 840 });
  viewOver('album:cratarr/al-42');
  keepPlace({ scroll: 120 });
  home();
  assert.equal(placeOf(), null, 'Home: the panel gone');
  history.back();
  assert.deepEqual(placeOf(), { scroll: 120 }, "the record's place, back with the trail");
  backOne();
  assert.deepEqual(placeOf(), { scroll: 840 }, "and Yours' under it");
});
