// Decision 0097's one reader test, the page's side: the record's words as the album view gives them (shared/words.json
// `expect.words`, the server's test checks its side against the same) shown as its rows, one per kind, in order.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const shared = JSON.parse(readFileSync(new URL('../../shared/words.json', import.meta.url), 'utf8'));
const { wordRows } = await import('../src/lib/wordRows.ts');

test("the record's rows are the shared fixture's words, a row per kind in the decision's order", () => {
  const rows = wordRows(shared.expect.words);
  const kinds = (shared.expect.words.kinds as { kind: string; words: { word: string }[] }[]).filter((k) => k.words.length);
  assert.deepEqual(
    rows.map((r) => r.kind),
    ['genre', 'style', 'mood', 'tag'].filter((k) => kinds.some((x) => x.kind === k)),
  );
  for (const r of rows) assert.deepEqual(r.words, kinds.find((k) => k.kind === r.kind)!.words.map((w) => w.word), r.kind);
  assert.deepEqual(rows.map((r) => r.label), ['Genre', 'Style', 'Mood', 'Tags'].slice(0, rows.length));
});

test('a kind without words has no row; no words, no rows', () => {
  assert.deepEqual(wordRows({ kinds: [{ kind: 'mood', hand: false, words: [] }], tracks: [] }), []);
  assert.deepEqual(wordRows(undefined), []);
  assert.deepEqual(
    wordRows({ kinds: [{ kind: 'tag', hand: true, words: [{ word: 'x', sources: ['hand'] }] }, { kind: 'genre', hand: false, words: [{ word: 'Dub', sources: ['files'] }] }], tracks: [] }).map((r) => r.kind),
    ['genre', 'tag'],
  );
});
