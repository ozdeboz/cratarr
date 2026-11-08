// A search whose words read as a crate comes in one step (the owner, testing 0.16.0 on the phone: "Trance 90s" said "no
// results", then the results, then the crate card on top pushing them down): its results wait for the words' reading and
// the card's count, which come together (lib/globalSearch, the real one).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { effects } from '../harness/pageGlobals.ts';

const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { 'content-type': 'application/json' } });
const nothing = { groups: {} };
const trance = { groups: { genre: { items: [{ kind: 'genre', subject: 'crate:genre:Trance', title: 'Trance', sub: '12 albums', score: 1, matched: [] }], total: 1 } } };
let card: ((r: Response) => void) | null = null;
globalThis.fetch = (async (input: string) => {
  const u = new URL(String(input), 'http://cratarr.test');
  if (u.pathname === '/api/crates/preview') return new Promise<Response>((r) => (card = r));
  if (u.pathname === '/api/search') return json(u.searchParams.get('q')?.toLowerCase() === 'trance' ? trance : nothing);
  return json({});
}) as typeof fetch;

const { capabilities } = await import('../../src/lib/capabilities.svelte.ts');
const { pageView } = await import('../../src/lib/pageView.svelte.ts');
const { globalAsked, globalSearch } = await import('../../src/lib/globalSearch.svelte.ts');
const reading = effects.find((e) => String(e).includes('readLine('))!;
const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
};

test("words read as a crate: the results wait for the reading and the crate card's count, then come with them", async () => {
  Object.assign(capabilities, { names: ['quick_crates'], older: false });
  pageView.query = 'Trance 90s';
  // The whole line's own answer finds nothing: "Nothing in Cratarr" if the results showed by it alone.
  globalSearch.found = { q: 'Trance 90s', r: nothing as never };
  reading();
  await settle();
  assert.equal(globalAsked(), false, 'not answered while the card is on its way');
  assert.ok(card, "the card's count asked with the reading");
  card(json({ source: 'cratarr', total: 44, rows: [], reasons: [], all_back: 0, covers: [] }));
  await settle();
  assert.equal(globalAsked(), true);
  assert.equal(globalSearch.crate?.preview?.total, 44, 'the card comes with the results');
});
