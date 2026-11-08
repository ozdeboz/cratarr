// The page's build id (web/buildId.ts): the same for two builds of one commit whose chunks came out under other names
// (main and the adapters' chunks naming each other, 2026-11-04), and another for any change in what the page is.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { bare, buildId } = await import('../buildId.ts');

/** A build whose main and jellyfin chunks name each other, under hashes `a` and `b`, with a public file beside them. */
const build = (a: string, b: string, body = 'play()', sw = 'self.oninstall=()=>{}') => [
  { fileName: `assets/main-${a}.js`, content: `import("./jellyfin-${b}.js");${body}` },
  { fileName: `assets/jellyfin-${b}.js`, content: `import{x}from"./main-${a}.js";x()` },
  { fileName: 'index.html', content: `<script type="module" src="/assets/main-${a}.js"></script>` },
  { fileName: 'assets/tape-WaSm1234.wasm', content: new Uint8Array([0, 97, 115, 109]) },
  { fileName: 'sw.js', content: sw },
];

test('a hashed name loses its hash; anything else keeps its name', () => {
  assert.equal(bare('assets/main-CBwqQcPE.js'), 'assets/main.js');
  assert.equal(bare('assets/decoder.worker-DJ_0HsDu.js'), 'assets/decoder.worker.js');
  assert.equal(bare('assets/index-Ab12-_xY.css'), 'assets/index.css');
  assert.equal(bare('sw.js'), 'sw.js');
  assert.equal(bare('icons/icon-maskable.png'), 'icons/icon-maskable.png');
});

test('two builds of one commit, their chunks under other names: one build', () => {
  assert.equal(buildId(build('CBwqQcPE', 'Xy12Ab34')), buildId(build('DJ_0HsDu', 'Qq98Zz76')));
});

test('the order the files come in does not matter', () => {
  const files = build('CBwqQcPE', 'Xy12Ab34');
  assert.equal(buildId(files), buildId([...files].reverse()));
});

test('any change in what the page is makes another build: its code, a public file, a binary', () => {
  const id = buildId(build('CBwqQcPE', 'Xy12Ab34'));
  assert.notEqual(buildId(build('CBwqQcPE', 'Xy12Ab34', 'pause()')), id);
  assert.notEqual(buildId(build('CBwqQcPE', 'Xy12Ab34', 'play()', 'self.oninstall=null')), id);
  const files = build('CBwqQcPE', 'Xy12Ab34');
  files[3] = { fileName: 'assets/tape-WaSm1234.wasm', content: new Uint8Array([0, 97, 115, 110]) };
  assert.notEqual(buildId(files), id);
});

test('twelve characters, safe in a file and an address', () => {
  assert.match(buildId(build('CBwqQcPE', 'Xy12Ab34')), /^[A-Za-z0-9_-]{12}$/);
});
