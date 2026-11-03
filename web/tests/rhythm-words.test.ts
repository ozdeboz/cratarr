// Rhythm in the crate sentence (0.15; the owner, 2026-11-02): "four on the floor" and a time signature ("4/4") read as
// terms of their own, made the server's `four` and `meter` conditions, said back in the sentence and read back from
// the conditions (lib/quickCrate).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { rhythmOf, readLine, chipOf, conditionsOf, sentenceOf, chipsOf } = await import('../src/lib/quickCrate.ts');

const said = (parts: { text: string; chip?: number; tight?: boolean }[]) => parts.map((p, k) => (k && !p.tight ? ' ' : '') + (p.chip !== undefined ? `[${p.text}]` : p.text)).join('');

test('four on the floor and a time signature read as terms; other numbers do not', () => {
  assert.deepEqual(rhythmOf('Four on the Floor'), { kind: 'four', value: 'yes', label: 'four on the floor' });
  assert.deepEqual(rhythmOf('four-on-the-floor'), { kind: 'four', value: 'yes', label: 'four on the floor' });
  assert.deepEqual(rhythmOf('4/4'), { kind: 'meter', value: '4/4', label: '4/4' });
  assert.deepEqual(rhythmOf('6/8'), { kind: 'meter', value: '6/8', label: '6/8' });
  assert.equal(rhythmOf('4/5'), null);
  assert.equal(rhythmOf('1984'), null);
});

test('a line reads them as one term each, the search not asked for them', async () => {
  const asked: string[] = [];
  const reads = await readLine('techno four on the floor 4/4', async (p) => (asked.push(p), [{ kind: 'genre', subject: 'g', title: 'Techno', score: 1, matched: [] }]), 2026);
  assert.deepEqual(
    reads.map((r) => [r.term, r.readings[0]?.kind]),
    [
      ['techno', 'genre'],
      ['four on the floor', 'four'],
      ['4/4', 'meter'],
    ],
  );
  assert.ok(!asked.some((p) => /floor|4\/4/.test(p)));
});

const chip = (term: string, not = false) => ({ ...chipOf(term, [rhythmOf(term)!])!, ...(not ? { not: true } : {}) });
const techno = { term: 'Techno', reading: { kind: 'genre' as const, value: 'Techno', label: 'Techno' }, others: [] };

test("they are the server's four and meter conditions, and said in the sentence", () => {
  const chips = [techno, chip('four on the floor'), chip('4/4'), chip('3/4', true)];
  assert.deepEqual(conditionsOf(chips), [
    { axis: 'genre', not: false, value: { kind: 'any_of', values: ['Techno'] } },
    { axis: 'meter', not: false, value: { kind: 'any_of', values: ['4/4'] } },
    { axis: 'four', not: false, value: { kind: 'yes' } },
    { axis: 'meter', not: true, value: { kind: 'any_of', values: ['3/4'] } },
  ]);
  assert.equal(said(sentenceOf(chips)), '[Techno], [four on the floor] in [4/4], but not in [3/4]');
});

test('the conditions read back as the same chips', () => {
  const chips = [techno, chip('four on the floor', true), chip('4/4')];
  const back = chipsOf(conditionsOf(chips));
  assert.deepEqual(back.rest, []);
  assert.deepEqual(
    back.chips.map((c) => [c.reading.kind, c.reading.value, !!c.not]),
    [
      ['genre', 'Techno', false],
      ['meter', '4/4', false],
      ['four', 'yes', true],
    ],
  );
});
