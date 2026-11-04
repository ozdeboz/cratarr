// One motion for the glass (lib/glassMotion; the owner, 2026-11-01: "a bottom up guarantee it will always animate
// smoothly"): its shapes' arithmetic, and the guarantee itself, that no glass part animates a size or a place, so no
// step of a motion lays the page out again.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { between, grownTo, insetOf, rectIn, sameRect, union } = await import('../src/lib/glassShape.ts');

const SRC = new URL('../src/', import.meta.url);
const read = (f: string) => readFileSync(new URL(f, SRC), 'utf8');

test('the shapes: in a frame, both held, between, and as a clip of another box', () => {
  const a = { x: 100, y: 10, w: 120, h: 48 };
  const b = { x: 40, y: 10, w: 180, h: 48 };
  assert.deepEqual(rectIn({ left: 110, top: 30, width: 120, height: 48 }, { left: 10, top: 20 }), a);
  assert.deepEqual(union(a, b), { x: 40, y: 10, w: 180, h: 48 });
  assert.deepEqual(between(a, b, 0.5), { x: 70, y: 10, w: 150, h: 48 });
  assert.ok(sameRect(a, { ...a, x: 100.3 }));
  assert.ok(!sameRect(a, b));
  // The old, narrower outline as a clip of the grown glass: cut on the left only.
  assert.equal(insetOf(a, b, 26), 'inset(0px 0px 0px 60px round 26px)');
  // A larger shape than the box: past it (below 0).
  assert.equal(insetOf(b, a, 26), 'inset(0px 0px 0px -60px round 26px)');
  assert.equal(grownTo(b, a), '0px 0px 0px -60px');
});

/** Each CSS rule of a component's style whose selector names one of `parts`, its selector and body. */
function rulesOf(src: string, parts: readonly string[]) {
  const style = src.slice(src.indexOf('<style'));
  const out: { sel: string; body: string }[] = [];
  for (const m of style.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sel = m[1].replace(/\/\*[\s\S]*?\*\//g, '').trim();
    if (parts.some((p) => sel.includes(p))) out.push({ sel, body: m[2] });
  }
  return out;
}

const LAYOUT = /\b(top|bottom|left|right|inset|width|height|min-width|max-width|min-height|max-height|margin[a-z-]*|padding[a-z-]*|flex[a-z-]*|gap|grid[a-z-]*|all)\b/;

test('no glass part animates a size or a place: its rules transition only transform, translate, opacity and the like', () => {
  const files: [string, string[]][] = [
    ['App.svelte', ['.page-row', '.bar', '.right']],
    ['lib/ContentControls.svelte', ['.pill', '.glass', '.shade', '.globe', 'button', '.field', '.work']],
    ['lib/NavigationControls.svelte', ['.lead', '.glass', '.shade', '.navpill', '.page-name']],
  ];
  for (const [file, parts] of files) {
    for (const { sel, body } of rulesOf(read(file), parts)) {
      for (const m of body.matchAll(/transition(?:-property)?\s*:([^;]*);/g)) {
        for (const item of m[1].split(',')) {
          const prop = item.trim().split(/\s+/)[0];
          assert.ok(!LAYOUT.test(prop), `${file} ${sel}: transitions ${prop}`);
        }
      }
      assert.ok(!/\banimation\s*:/.test(body), `${file} ${sel}: a CSS animation (lib/glassMotion moves the glass)`);
    }
  }
});

test('the glass parts take no Svelte transition that sizes them, and their motion moves only what composites', () => {
  for (const file of ['lib/ContentControls.svelte', 'lib/NavigationControls.svelte']) {
    const src = read(file);
    assert.ok(!/\b(slide|scale|draw|crossfade)\b[^\n]*from 'svelte\/transition'/.test(src), `${file}: a sizing transition imported`);
    assert.ok(!/transition:slide|in:slide|out:slide/.test(src), `${file}: slides a size`);
    assert.match(src, /glassPart\(/, `${file}: not a glass part`);
  }
  const app = read('App.svelte');
  assert.match(app, /class="bar"[^>]*glassPart\(/, 'the bar is a glass part');
  assert.ok(!/class="more-tabs"[\s\S]{0,200}transition:slide/.test(app), "More's tabs slide a size");
  // lib/glassMotion's keyframes: clip-path, opacity and translate, and the shade's own edges (`inset`, an empty layer
  // laid out alone: stretched by a scale its round ends went oval, the owner 2026-11-02).
  const motion = read('lib/glassMotion.ts');
  const animated = [
    ...motion.matchAll(/\.animate\(\s*\[([\s\S]*?)\],/g),
    ...motion.matchAll(/\bbegin\(\s*[\w.]+,\s*\[([\s\S]*?)\],/g),
    ...motion.matchAll(/\bframes(?:: Keyframe\[\])? = ([\s\S]*?\]);\r?\n/g),
    ...motion.matchAll(/\bframes\.splice\([^{]*(\{[^}]*\})\)/g),
  ];
  assert.ok(animated.length >= 4, 'its moves found');
  for (const frames of animated) {
    for (const k of frames[1].matchAll(/([a-zA-Z]+):/g)) {
      // `offset` is where a keyframe lies in the move, not a property moved.
      assert.ok(['clipPath', 'opacity', 'translate', 'inset', 'offset'].includes(k[1]), `glassMotion animates ${k[1]}`);
    }
  }
});
