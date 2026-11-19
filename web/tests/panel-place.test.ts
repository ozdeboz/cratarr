// F25 with SB15: every panel keeps its place on back, by one rule. `placeBack` (lib/placeBack), the one retry App's page and every
// panel step use, on a fake scroller and node:test's mock timers; and the source's one attachment (lib/keepsPlace) on every panel.
import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { placeBack } from '../src/lib/placeBack.ts';

/** Time advanced in steps of 10 ms, each try's next one scheduled in turn. */
function advance(ms: number) {
  for (let t = 0; t < ms; t += 10) mock.timers.tick(10);
}

const src = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

/** A list of `height` px in a window of `view` px: where it is asked to go, it goes as far as it can. */
function scroller(height: number, view = 600) {
  const s = { height, top: 0, asked: 0, target: new EventTarget() };
  return {
    s,
    at: { scrollTo: (y: number) => ((s.asked += 1), (s.top = Math.max(0, Math.min(y, s.height - view)))), at: () => s.top, long: () => s.height, target: s.target },
  };
}

test('F25.4: the place lands once the list is long enough, by the fifth try, and nothing moves after', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const { s, at } = scroller(600);
    placeBack(at, 840, { every: 100, tries: 40 });
    advance(350);
    s.height = 2000;
    advance(50);
    assert.deepEqual([s.top, s.asked], [840, 5], 'tried at 0, 100, 200, 300 and 400 ms');
    advance(5000);
    assert.equal(s.asked, 5, "landed: no more tries");
  } finally {
    mock.timers.reset();
  }
});

test('F25.6 (a), F25.7: after forty tries it stays where it got, and nothing moves later; a shorter list keeps its end, or its top', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const { s, at } = scroller(1500);
    placeBack(at, 1800);
    advance(10_000);
    assert.deepEqual([s.top, s.asked], [900, 41], 'at the end of a list shorter now, forty tries after the first');
    s.height = 4000;
    advance(10_000);
    assert.equal(s.top, 900, 'no jump seconds later');
    const short = scroller(300);
    placeBack(short.at, 2400);
    advance(10_000);
    assert.equal(short.s.top, 0, 'shorter than its window: its top, the only place there is');
  } finally {
    mock.timers.reset();
  }
});

test('F25.6 (b): the hand stops a place coming back at once, and its place stands', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    for (const hand of ['wheel', 'touchstart', 'pointerdown', 'keydown']) {
      const { s, at } = scroller(600);
      const cancel = placeBack(at, 2400);
      advance(150);
      s.target.dispatchEvent(new Event(hand));
      const asked = s.asked;
      s.height = 4000;
      advance(5000);
      assert.deepEqual([s.asked, s.top], [asked, 0], hand);
      cancel();
    }
    const { s, at } = scroller(600);
    const cancel = placeBack(at, 2400);
    cancel();
    s.height = 4000;
    advance(5000);
    assert.equal(s.asked, 1, 'cancelled (its step left): no more tries');
  } finally {
    mock.timers.reset();
  }
});

test('F25.2: every panel attaches the one place keeper to its list, and none brings its place back by its own code', () => {
  for (const f of ['lib/RequestsPanel.svelte', 'lib/HealthPanel.svelte', 'lib/PlaylistView.svelte', 'lib/InsightsPanel.svelte', 'lib/DownloadsPanel.svelte', 'lib/SettingsPanel.svelte']) {
    const s = src(f);
    assert.match(s, /<div class="scroll"[^>]*\{@attach keepsPlace\(/, f);
    assert.doesNotMatch(s, /placeBack|history\.state\?*\.scroll/, `${f}: no place of its own`);
    for (const m of s.matchAll(/scrollTop = /g)) {
      const line = s.slice(s.lastIndexOf('\n', m.index) + 1, s.indexOf('\n', m.index));
      assert.match(line, /reachedBy\) !== 'back'/, `${f}: a reset of its own only on a forward step: ${line}`);
    }
  }
  assert.match(src('lib/HealthPanel.svelte'), /keepsPlace\(\{ open: \(\) => openKey, reopen: \(k\) => \(openKey = k\) \}\)/, "Health's open row rides with its place");
});

test('F25.4: App and the panels bring a place back by one function; App has no retry of its own', () => {
  const app = src('App.svelte');
  assert.match(app, /placeBack\(\{ scrollTo: \(to\) => window\.scrollTo\(0, to\)/);
  assert.doesNotMatch(app, /n\+\+ < 40/, "App's own retry gone");
  assert.match(src('lib/keepsPlace.ts'), /placeBack\(\{ scrollTo: \(y\) => el\.scrollTo\(0, y\)/);
});

test('F25.3, F25.8: a viewed-again never undoes a restore as the list mounts; Health restores its open row after its own clearing', () => {
  const view = src('lib/PlaylistView.svelte');
  assert.match(view, /let againSeen = untrack\(\(\) => panel\.again\);\n  \$effect\(\(\) => \{\n    const again = panel\.again;\n    if \(again === againSeen\) return;/);
  const health = src('lib/HealthPanel.svelte');
  assert.match(health, /void problem;\n    openKey = untrack\(\(\) => placeOf\(\)\?\.open \?\? null\);/);
  const settings = src('lib/SettingsPanel.svelte');
  assert.match(settings, /if \(scrollEl && untrack\(reachedBy\) !== 'back'\) scrollEl\.scrollTop = 0;/, 'a part chosen anew starts at its top; back does not');
});
