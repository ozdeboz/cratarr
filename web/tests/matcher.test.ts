// The page's matcher (lib/matcher, matcher.wasm) held to the cases the server's is held to
// (server/rules/tests/vectors/matcher.json): each query against one name, and lists in their order.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { instantiateMatcher, TextMatcher, fold } = await import('../src/lib/matcher.ts');
const matcher = await instantiateMatcher(readFileSync(new URL('../src/lib/wasm/matcher.wasm', import.meta.url)));
type Name = { text: string; aliases?: string[] };
const vectors = JSON.parse(readFileSync(new URL('../../server/rules/tests/vectors/matcher.json', import.meta.url), 'utf8')) as {
  score: { q: string; text: string; aliases?: string[]; how: string | null }[];
  order: { q: string; names: Name[]; first: number[]; how: string[]; absent?: number[] }[];
};

test('each name is found as the shared cases say', () => {
  assert.ok(vectors.score.length >= 20);
  for (const c of vectors.score) {
    const handle = matcher.index([{ text: c.text, aliases: c.aliases }]);
    assert.ok(handle > 0);
    const hit = matcher.query(handle, c.q, 1)[0] ?? null;
    matcher.drop(handle);
    assert.equal(hit?.how ?? null, c.how, `${c.q} for ${c.text}: ${JSON.stringify(hit)}`);
    if (hit) assert.ok(hit.score >= 0 && hit.score <= 1, `${c.q} for ${c.text}: ${hit.score}`);
  }
});

test('names rank as the shared cases say', () => {
  for (const c of vectors.order) {
    const handle = matcher.index(c.names);
    const hits = matcher.query(handle, c.q, 100);
    matcher.drop(handle);
    assert.deepEqual(
      hits.slice(0, c.first.length).map((h) => h.i),
      c.first,
      `${c.q}: ${JSON.stringify(hits)}`,
    );
    hits.forEach((h, k) => k < c.how.length && assert.equal(h.how, c.how[k], `${c.q}: ${JSON.stringify(hits)}`));
    for (const a of c.absent ?? []) assert.ok(hits.every((h) => h.i !== a), `${c.q} found ${c.names[a].text}`);
  }
});

test('texts are folded as the server folds them before the module reads them', () => {
  assert.equal(fold('Björk'), 'bjork');
  assert.equal(fold('Sigur Rós'), 'sigur ros');
  assert.equal(fold('ＡＢＣ ﬁ Ⅳ'), 'abc fi iv');
});

test("an item's texts are one name, each also its own", () => {
  const m = new TextMatcher(matcher, 50);
  // Across its fields: every word begins one of the name's.
  assert.equal(m.hit('kind blue miles', ['Kind of Blue', 'Miles Davis'])?.how, 'prefix');
  // One field whole: that field as an alias.
  assert.equal(m.hit('miles davis', ['Kind of Blue', 'Miles Davis'])?.how, 'alias');
  assert.equal(m.hit('hiphop', ['Hip Hop'])?.how, 'exact');
  assert.equal(m.hit('zzzz', ['Hip Hop']), null);
  // Empty texts and an empty query find nothing.
  assert.equal(m.hit('hip', ['', '  ']), null);
  assert.equal(m.hit(' ', ['Hip Hop']), null);
});

test('indexes are let go past their most, and asked again as before', () => {
  const m = new TextMatcher(matcher, 3);
  for (const t of ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon']) assert.equal(m.hit(t.toLowerCase(), [t])?.how, 'exact');
  assert.equal(m.hit('alpha', ['Alpha'])?.how, 'exact');
  m.clear();
  assert.equal(m.hit('beta', ['Beta'])?.how, 'exact');
});
