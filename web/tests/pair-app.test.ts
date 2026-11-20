// QR04 (QR01, QR02 the server's): Account's Devices end with "+ Pair an app", which asks the password again (OQ-B2 (a)) and opens
// its own page, the QR and the code and nothing else; once paired it steps back to the list, where the new sign-in shows. Its rule
// runs (lib/pairing); the pages are read. Red first: none of it was there at 7da786c91. The whole path is the e2e
// `an_app_pairs_with_a_code`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PAIR_ALPHABET, pairStep } from '../src/lib/pairing.ts';

const src = (f: string) => readFileSync(new URL(`../src/lib/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test("the alphabet is QR01's: 32 signs, none of 0, O, 1 or I", () => {
  assert.equal(PAIR_ALPHABET.length, 32);
  assert.equal(new Set(PAIR_ALPHABET).size, 32);
  for (const c of '0O1I') assert.ok(!PAIR_ALPHABET.includes(c), c);
});

test('the page shows while the code waits, and steps back once paired, or once the code is gone', () => {
  assert.equal(pairStep(null), 'show', 'not read yet');
  assert.equal(pairStep({ state: 'waiting' }), 'show');
  assert.equal(pairStep({ state: 'paired', device: 'Cratarr app · Phone' }), 'back');
  assert.equal(pairStep({ state: 'none' }), 'back');
});

test('"+ Pair an app" is the last row of Devices, asks the password again, then opens its page', () => {
  const account = src('settings/AccountPart.svelte');
  const devices = account.slice(account.indexOf('<Group title="Devices">'), account.indexOf('</Group>', account.indexOf('<Group title="Devices">')));
  assert.match(devices, /<Item name="Pair an app" kind="add" onclick=\{\(\) => \(\(error = null\), \(current = ''\), \(pairing = true\)\)\} \/>\n    \{\/if\}\n  $/, 'the last row');
  assert.ok(account.includes('<FormSheet title="Pair an app" submitLabel="Pair" canSubmit={!!current} {busy} {error} onsubmit={pair}'));
  assert.match(account, /pairs\.code = \(await pairCode\(current\)\)\.code;\n      pairing = false;\n      openThing\('account', 'pair'\);/);
  assert.match(account, /\} finally \{\n      busy = false;\n      current = '';\n    \}/, 'the password never kept');
  assert.ok(account.includes("{:else if thing === 'pair'}\n  <PairPage />"));
});

test('its page: the QR and the code, nothing else; asked on a timer; once paired, back to the list with Signed in read again', () => {
  const page = src('settings/PairPage.svelte');
  const markup = page.slice(page.indexOf('</script>'), page.indexOf('<style>'));
  assert.deepEqual([...markup.matchAll(/<(\w+)[\s>]/g)].map((m) => m[1]), ['div', 'img', 'p'], 'the QR and the code alone');
  assert.ok(markup.includes('src={`${PAIR_QR}?code=${encodeURIComponent(pairs.code)}`}'));
  assert.match(page, /return pollEvery\(2_000, pairState, \(r\) => \{\n      if \(!r\.ok \|\| pairStep\(r\.value\) === 'show'\) return;\n      pairs\.code = null;\n      void loadMe\(\);\n      backFromThing\('account'\);/);
  assert.ok(page.includes("if (!untrack(() => pairs.code)) return void backFromThing('account');"), 'opened with no code: back');
  const api = src('api.ts');
  assert.ok(api.includes("post<{ code: string }>('/api/pair', { password })"));
  assert.ok(api.includes("quietly<import('./pairing').PairState>('/api/pair')"));
  assert.ok(api.includes("export const PAIR_QR = '/api/pair/qr';"));
});
