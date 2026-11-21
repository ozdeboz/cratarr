// Effects that read the $state they write (F39, F43; the owner, 2026-11-19: "fixed durably and robustly"): each one found is
// reviewed below with why it settles, or this fails. P/split-tools/effscan.py's rule, with comments and strings blanked first
// (their words are no reads). Its blind spots, as the review of 1.2 lists them: writes through a call or another module's
// $state, `$effect(fn)` and expression effects, `{@attach}`, and two effects feeding each other.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const SRC = new URL('../src/', import.meta.url);
/** `file|state` → [how many effects, why each settles]. No line numbers: they drift. */
const SETTLES: Record<string, [number, string]> = {
  'App.svelte|moreOpen': [1, 'written in the keydown handler only'],
  'App.svelte|page': [1, 'the plain openedAway is set before the write: once'],
  'Discover.svelte|seenEnd': [1, 'guarded by t.id !== seenEnd'],
  'Home.svelte|skyIn': [1, 'returns when true; only ever set true'],
  'Library.svelte|switching': [1, 'set false only while true'],
  'lib/AlbumShelf.svelte|reach': [1, 'a timer adds blocks until reach >= most'],
  'lib/AlbumShelf.svelte|selecting': [1, 'written in the keydown handler only'],
  'lib/ContentControls.svelte|hidden': [1, 'written in the pointerdown handler only'],
  'lib/HealthRenumber.svelte|taskId': [1, 'guarded by t.id !== taskId, then null'],
  'lib/NotForSheet.svelte|axes': [1, "written in the answer's callbacks only"],
  'lib/PlaylistMenu.svelte|asked': [1, 'a local const asked shadows the $state'],
  'lib/PlaylistView.svelte|byOf': [1, "read only in the answers' callbacks; reruns on the strings albumFrom/albumByKey (F39)"],
  'lib/ProgressBar.svelte|value': [1, "the creep writes in setInterval; the bar's read is untracked"],
  'lib/RecordReferences.svelte|rows': [1, 'the || rows guard; set once by the answer'],
  'lib/SeekWave.svelte|after': [2, 'set to null only when it matches the player; the other write is in a timer'],
  'lib/SettingsPanel.svelte|scrollEl': [1, 'scrollTop is a DOM property, not state'],
  'lib/sheet/Record.svelte|coverTask': [1, 'guarded by t.id !== coverTask, then null'],
  'lib/settings/LibrariesPart.svelte|statuses': [1, 'a per-path guard; written by the answer'],
};

/** Comments and string literals blanked, newlines kept: their words are no reads. */
const blank = (s: string) =>
  s.replace(/\/\*[\s\S]*?\*\/|<!--[\s\S]*?-->|(?<![:\\])\/\/[^\n]*|'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"/g, (m) => m.replace(/[^\n]/g, ' '));
/** The index of the bracket closing the one at `at`. */
function closing(s: string, at: number): number {
  const [o, c] = s[at] === '(' ? ['(', ')'] : ['{', '}'];
  for (let k = at, d = 0; k < s.length; k++) if (s[k] === o) d++;
  else if (s[k] === c && --d === 0) return k;
  return s.length;
}
/** The effect's own pass: nested `=> { … }` bodies (they run later) and `untrack( … )` cut out. */
function ownPass(b: string): string {
  const re = /=>\s*\{|untrack\(/g;
  let out = '';
  let i = 0;
  for (let m; (re.lastIndex = i), (m = re.exec(b)); i = closing(b, m.index + m[0].length - 1) + 1) out += b.slice(i, m.index);
  return out + b.slice(i);
}
/** Every effect of every file, with each `$state` it both writes and reads: `file|state` → how many effects. */
function found(files: Iterable<[string, string]>): Map<string, number> {
  const n = new Map<string, number>();
  for (const [f, text] of files) {
    const s = blank(text);
    const states = new Set([...s.matchAll(/\blet\s+(\w+)\s*=\s*\$state/g)].map((m) => m[1]));
    for (const m of s.matchAll(/\$effect(?:\.pre)?\(\s*\(\)\s*=>\s*\{/g)) {
      const open = m.index! + m[0].length - 1;
      const b = ownPass(s.slice(open + 1, closing(s, open)));
      for (const v of states) {
        const count = (re: string) => b.match(new RegExp(re, 'g'))?.length ?? 0;
        const path = `(?<![\\w.])${v}(?:\\s*\\[[^\\]]*\\]|\\.\\w+)*\\s*`;
        // A plain `=` only writes; `+=`, `||=`, `??=`, `++` and `--` read what they write as well.
        const plain = count(`${path}=(?![=>])`);
        const both = count(`${path}(?:\\*\\*|<<|>>>?|\\|\\||&&|\\?\\?|[-+*/%|&^])=`) + count(`${path}(?:\\+\\+|--)|(?:\\+\\+|--)\\s*${v}\\b`);
        const reads = count(`(?<![\\w.])${v}\\b`) - plain;
        if (plain + both && reads > 0) n.set(`${f}|${v}`, (n.get(`${f}|${v}`) ?? 0) + 1);
      }
    }
  }
  return n;
}
function* sources(): Generator<[string, string]> {
  for (const f of readdirSync(SRC, { recursive: true }).map((x) => String(x).replace(/\\/g, '/'))) {
    if (/\.svelte(\.ts)?$/.test(f)) yield [f, readFileSync(new URL(f, SRC), 'utf8')];
  }
}

test('every effect that reads the $state it writes is a reviewed one that settles', () => {
  const now = found(sources());
  assert.deepEqual(
    [...now].filter(([k, n]) => SETTLES[k]?.[0] !== n).map(([k, n]) => `${k} (${n})`),
    [],
    'a loop, or say in SETTLES why it settles',
  );
  assert.deepEqual(Object.keys(SETTLES).filter((k) => !now.has(k)), [], 'reviewed but no longer found: take it off');
});

test('the scan sees a new effect that reads and writes one $state, and not one that reads a local or writes in a callback', () => {
  const page = (effect: string) => [['lib/X.svelte', `<script lang="ts">\n  let shown = $state(false);\n  $effect(() => {\n${effect}\n  });\n</script>`]] as [string, string][];
  // H2's shape: the write, then the read that makes it a dependency.
  assert.deepEqual([...found(page('const c = crate; shown = sure(c); if (!c || shown) return; ask(c).then((d) => (shown = sure(c, d)));'))], [['lib/X.svelte|shown', 1]]);
  assert.deepEqual([...found(page('shown = !shown;'))], [['lib/X.svelte|shown', 1]]);
  // A write that reads what it writes: `||=`, `+=`, `++`.
  assert.deepEqual([...found(page('items.push(1); shown ||= items.length > 3;'))], [['lib/X.svelte|shown', 1]]);
  const counted = (effect: string) => [['lib/Y.svelte', `<script lang="ts">\n  let n = $state(0);\n  $effect(() => {\n${effect}\n  });\n</script>`]] as [string, string][];
  for (const e of ['n += 1;', 'n++;', '--n;', 'if (open) n = n + 1;']) assert.deepEqual([...found(counted(e))], [['lib/Y.svelte|n', 1]], e);
  assert.deepEqual([...found(counted('if (open) n = 0;'))], []);
  // Its fix: a local is read, never the state.
  assert.deepEqual([...found(page('const c = crate; const s = sure(c); shown = s; if (!c || s) return; ask(c).then((d) => (shown = sure(c, d)));'))], []);
  // Read only in a later callback, or untracked; the name inside a comment or a string.
  assert.deepEqual([...found(page('ask().then(() => { if (!shown) shown = true; });'))], []);
  assert.deepEqual([...found(page('shown = untrack(() => !shown);'))], []);
  assert.deepEqual([...found(page("// shown is read\n    shown = 'shown' in x;"))], []);
});
