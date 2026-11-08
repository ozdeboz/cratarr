// No page asks whether it is offline (the owner's North Star, 2026-11-07; docs/architecture.md's Reads door): reads go through
// lib/api's `read`, and lib/deviceIndex alone says where they come from. So `reach.away` may tell a control or a word what to do,
// and a poller when to ask, and may not choose what a page reads. Each use stands here with what it is for: a new one fails until
// someone has said so, and what it chooses is then read in this list, not found by accident.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const SRC = new URL('../src', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');

function sources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (name !== 'generated') sources(path, out);
    } else if (/\.(ts|svelte)$/.test(name)) out.push(path);
  }
  return out;
}

/** Code with no line that is all comment. */
const code = (text: string) =>
  text
    .split(/\r?\n/)
    .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
    .join('\n');

/** How often a file reads whether Cratarr is out of reach: `reach.away`, and lib/away's `away()` where it is imported. */
function uses(text: string) {
  const asked = text.match(/\breach\.away\b/g)?.length ?? 0;
  const imported = /import \{[^}]*\baway\b[^}]*\} from '[./a-zA-Z]*away\.svelte'/.test(text);
  return asked + (imported ? (text.match(/(?<![\w.])away\(\)/g)?.length ?? 0) : 0);
}

/** What may read it, how often, and what for. Not one of these chooses what a page reads. */
const ALLOWED: Record<string, [number, string]> = {
  'App.svelte': [2, 'which page the app opens on away from home; the quick crate card offered'],
  'Home.svelte': [2, 'Radio offered, and the moment offered with nothing of it here'],
  'Playlists.svelte': [2, 'New playlist offered, and the word that it needs Cratarr'],
  'lib/DownloadsPanel.svelte': [2, 'Retry offered, and its word'],
  'lib/MenuSheet.svelte': [2, 'which of a menu\'s entries are offered'],
  'lib/MenuTable.svelte': [2, 'which of a menu\'s entries are offered, and the word for the rest'],
  'lib/PlaylistView.svelte': [4, 'which edits can wait in the outbox'],
  'lib/away.svelte.ts': [1, 'what plays goes by the first failed request, as a hold does: the marks go by the door'],
  'lib/capabilities.svelte.ts': [2, 'a poller: asked again on the way back'],
  'lib/changes.svelte.ts': [1, 'a poller: not asked while away'],
  'lib/events.svelte.ts': [1, 'a poller: how often it knocks'],
  'lib/later.svelte.ts': [2, 'the outbox: sent when Cratarr answers'],
  'lib/live.svelte.ts': [1, 'a poller: how often it knocks'],
  'lib/localPlayer.svelte.ts': [2, 'whether the queue kept here is ahead of Cratarr'],
  'lib/momentHere.svelte.ts': [2, 'whether the weather may be asked of anyone: only with Cratarr away'],
  'lib/output.svelte.ts': [2, 'what plays: picked from what is on the device'],
  'lib/outputNow.svelte.ts': [1, 'where music plays: here while Cratarr is away with another player chosen (0.16.1, the owner: "Play here instead")'],
  'lib/play.svelte.ts': [1, 'what plays: picked from what is on the device'],
  'lib/queueHistory.svelte.ts': [1, 'whose list a queue is in'],
  'lib/shelfStore.svelte.ts': [1, 'a poller: not asked while away'],
  'lib/snippetPlayer.svelte.ts': [1, 'Snippets offered'],
};

/** Where it is read to say what it is: the words themselves. */
const ITS_OWN = new Set(['lib/reach.svelte.ts', 'lib/deviceIndex.svelte.ts']);

test('reach.away chooses no page\'s data: each use is listed with what it is for, and a new one fails here', () => {
  const found: Record<string, number> = {};
  for (const path of sources(SRC)) {
    const name = relative(SRC, path).split(sep).join('/');
    const n = uses(code(readFileSync(path, 'utf8')));
    if (n && !ITS_OWN.has(name)) found[name] = n;
  }
  const listed = Object.fromEntries(Object.entries(ALLOWED).map(([name, [n]]) => [name, n]));
  assert.deepEqual(found, listed);
});

test('the counting sees what it must: a read chosen by it, either way of asking, and not a comment', () => {
  assert.equal(uses(code('const rows = reach.away ? await fromHere() : await getAlbums();')), 1);
  assert.equal(uses(code("import { away } from './lib/away.svelte';\nconst rows = away() ? a() : b();")), 1);
  assert.equal(uses(code('function away() {}\naway();')), 0, 'a function of its own by that name is not it');
  assert.equal(uses(code('// reach.away says so')), 0);
});
