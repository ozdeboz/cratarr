// Space plays and pauses everywhere in the page (the owner, 2026-11-08: "space should work everywhere"), where the arrows would
// go, except on a button or link reached by the keyboard, which Space presses; one clicked keeps no Space (lib/keyWind, the real one).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import '../harness/pageGlobals.ts';

/** An element as `closest` and `matches` find it: the selectors it answers to, and whether the keyboard brought the focus. */
class Element {
  is: string[];
  keyboard: boolean;
  constructor(is: string[], keyboard = false) {
    this.is = is;
    this.keyboard = keyboard;
  }
  closest(sel: string) {
    return sel.split(',').some((s) => this.is.includes(s.trim())) ? this : null;
  }
  matches(sel: string) {
    return sel === ':focus-visible' && this.keyboard;
  }
}
(globalThis as Record<string, unknown>).Element = Element;

const { keepsSpace } = await import('../../src/lib/keyWind.ts');

test("Space is the player's but where the focus keeps it", () => {
  assert.equal(keepsSpace(new Element([])), false, 'the page: play and pause');
  assert.equal(keepsSpace(new Element(['button'])), false, 'a button clicked: play and pause, not Next again');
  assert.equal(keepsSpace(new Element(['button'], true)), true, 'a button reached by the keyboard: pressed');
  assert.equal(keepsSpace(new Element(['input'])), true, 'a field types it');
  assert.equal(keepsSpace(new Element(['[role="slider"]'])), true);
  assert.equal(keepsSpace(null), false);
});

test('Space plays and pauses, once per press, the page not scrolled, nor with a modifier or a sheet open', () => {
  const src = readFileSync(new URL('../../src/lib/keyWind.ts', import.meta.url), 'utf8');
  const space = src.slice(src.indexOf('const space = '), src.indexOf('const lift = '));
  assert.match(src, /if \(e\.key === ' '\) return space\(e\);/);
  for (const part of ['e.shiftKey', 'keepsSpace(e.target)', '!playing()', "'[aria-modal=\"true\"], dialog[open]'", 'e.preventDefault()', 'if (e.repeat) return;', 'playPause();']) {
    assert.ok(space.includes(part), part);
  }
  assert.ok(space.indexOf('e.preventDefault()') < space.indexOf('if (e.repeat) return;'), 'a held key scrolls nothing either');
});
