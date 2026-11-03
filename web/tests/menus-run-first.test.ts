// A menu runs what was chosen, then closes (the owner, 2026-11-02: Go to album in a record's menu did nothing). Its
// parent clears what the menu was opened for as it closes, and an action handed in that reads it then finds nothing
// (a crate's record: "Cannot read properties of null"). No component with an `onclose` calls `onclose()` and then
// one of its own props (or an item of a list prop) in the same handler.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const src = new URL('../src/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const files = (d: string): string[] => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? files(join(d, f)) : f.endsWith('.svelte') ? [join(d, f)] : []));

/** What a handler does after its `onclose()`: on its line, to the end of the expression (`(onclose(), x())}`); a
 *  handler written over lines (`onclose();` alone), to the end of its block. */
function after(lines: string[], i: number, at: number): string {
  const rest = lines[i].slice(at);
  if (/\S/.test(rest.replace(/^;\s*$/, ''))) return rest.split(/\)\s*\}/)[0];
  return lines
    .slice(i + 1, i + 4)
    .join('\n')
    .split(/^\s*\}/m)[0];
}

test('no menu closes before running an action its parent handed it', () => {
  const found: string[] = [];
  for (const f of files(src)) {
    const s = readFileSync(f, 'utf8');
    const decl = s.match(/let \{([\s\S]*?)\}\s*:\s*\{[\s\S]*?\}\s*=\s*\$props\(\)/);
    if (!decl || !/\bonclose\b/.test(decl[1])) continue;
    const props = decl[1]
      .split(',')
      .map((p) => p.trim().split(/[=:]/)[0].trim())
      .filter((p) => /^[a-z]\w*$/.test(p) && p !== 'onclose' && p !== 'children');
    for (const m of s.matchAll(/\{#each (\w+) as (\w+)/g)) if (props.includes(m[1])) props.push(m[2]);
    const lines = s.split(/\r?\n/);
    lines.forEach((line, i) => {
      for (const m of line.matchAll(/onclose\(\)/g)) {
        const rest = after(lines, i, (m.index ?? 0) + m[0].length);
        const called = props.find((p) => new RegExp(`\\b${p}\\b\\??\\.?(\\w+\\??\\.?)?\\(`).test(rest));
        if (called) found.push(`${f.slice(src.length)}:${i + 1}: ${called}`);
      }
    });
  }
  assert.deepEqual(found, []);
});
