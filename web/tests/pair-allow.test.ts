// QR07, the page's Allow sheet (SCENARIOS-P2-S2 QR07.1–QR07.6; QR06 is the server's): on its pure view model (lib/pairing) and the
// sheet's source (lib/PairAllowSheet), which takes a whole app to run. Red first: none of it was there at 7da786c91. QR07.7, the
// whole path, is the e2e `a_browser_is_allowed_by_another_device_end_to_end`, at the run's close.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { allowCodeIn, allowShows, askLine, DEAD_CODE, typedCode } from '../src/lib/pairing.ts';

const src = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const sheet = src('lib/PairAllowSheet.svelte');
const ASK = { device: 'Firefox on Windows', asked: 1_000_000, home: false };

test('the_link_opens_the_allow_sheet', () => {
  // QR07.1: the QR's link is the page's address; its code opens the sheet, read as typed.
  assert.equal(allowCodeIn('/pair/allow', '?code=K7M2QX'), 'K7M2QX');
  assert.equal(allowCodeIn('/pair/allow/', '?code=k7m2qx'), 'K7M2QX');
  assert.equal(allowCodeIn('/pair/allow', '?code=K7M2Q'), null, 'no code: no sheet');
  assert.equal(allowCodeIn('/', '?code=K7M2QX'), null);
  const app = src('App.svelte');
  assert.match(app, /const asking = allowCodeIn\(location\.pathname, location\.search\);\n    if \(asking\) \{\n      pairs\.allow = asking;/);
  assert.ok(app.includes('<PairAllowSheet code={pairs.allow} onclose={() => (pairs.allow = null)} />'));
  // The sheet every question has: what it does, the device line (OQ-B1 (a)), Allow and Refuse.
  assert.ok(sheet.includes('title="This signs in a browser as you"'));
  assert.equal(askLine(ASK, 1_000_120), 'Firefox on Windows · 2 min ago · away from home');
  assert.equal(askLine({ ...ASK, home: true }, 1_000_010), 'Firefox on Windows · just now · at home');
  assert.ok(sheet.includes('<p class="text">{askLine(read, Date.now() / 1000)}</p>'));
  // Never: Allow pressed by opening; a sheet without the device line.
  assert.ok(sheet.includes('<MenuEntry icon="check" label="Allow" onclick={() => (typing = true)} />'));
  assert.doesNotMatch(sheet.slice(sheet.indexOf('<script'), sheet.indexOf('</script>')), /^\s*(?:void )?allow\(\);?$/m, 'nothing allows as it opens');
  assert.equal(allowShows(ASK, false), 'ask');
  assert.equal(allowShows(null, false), 'reading', 'no Allow before what asked is read');
});

test('allow_and_refuse_each_send_once', () => {
  // QR07.2: one request however often pressed; the sheet closes; no notice after; Signed in read again for when next shown.
  assert.match(sheet, /async function allow\(\) \{\n    if \(busy \|\| !password\) return;\n    busy = true;/);
  assert.match(sheet, /async function refuse\(\) \{\n    if \(busy\) return;\n    busy = true;/);
  assert.match(sheet, /await pairAllow\(asked, password\);\n      void loadMe\(\);\n      onclose\(\);/);
  assert.match(sheet, /await pairRefuse\(asked\);\n      onclose\(\);/);
  assert.doesNotMatch(sheet, /toast\.show\(/, 'no success notice');
  assert.ok(sheet.includes('<MenuEntry icon="ban" label="Refuse" danger onclick={() => void refuse()} />'));
  const api = src('lib/api.ts');
  assert.ok(api.includes("post<unknown>('/api/pair/allow', { code, password })"));
  assert.ok(api.includes("post<unknown>('/api/pair/refuse', { code })"));
});

test('the_allow_sheet_asks_the_password_again', () => {
  // QR07.3 (OQ-B2 (a)): the password row Settings has, in the sheet every password is typed in; the server's words as they are.
  assert.ok(sheet.includes('<label>Your password<input type="password" bind:value={password} autocomplete="current-password" autofocus /></label>'));
  assert.ok(sheet.includes('<FormSheet title="Allow" submitLabel="Allow" canSubmit={!!password} {busy} {error}'));
  assert.ok(sheet.includes('error = e instanceof Error ? e.message : String(e);'));
  // Never: the password kept by the page after the request.
  assert.match(sheet, /\} finally \{\n      busy = false;\n      password = '';\n    \}/);
});

test('a_dead_code_shows_one_line', () => {
  // QR07.4: unknown, used, refused, expired, or the server restarted: one line, no Allow, nothing telling one from another.
  assert.equal(allowShows('dead', false), 'dead');
  assert.equal(DEAD_CODE, "That code didn't work.");
  assert.match(sheet, /\{:else if shows === 'dead'\}\n      <p class="text">\{DEAD_CODE\}<\/p>\n    \{:else if/);
  assert.ok(sheet.includes("() => (read = 'dead'),"), 'any failure to read it is the one line');
});

test('allow_a_device_takes_a_typed_code', () => {
  // QR07.5: in Account's Devices beside Pair an app; shown in upper case as typed; fewer than six signs, or one outside the
  // alphabet, does not send.
  assert.deepEqual(typedCode('k7m2qx'), { shown: 'K7M2QX', code: 'K7M2QX' });
  assert.deepEqual(typedCode('K7M2Q'), { shown: 'K7M2Q', code: null });
  assert.deepEqual(typedCode('K7M2QO'), { shown: 'K7M2QO', code: null }, 'an O is no sign of the alphabet');
  assert.deepEqual(typedCode('k7m 2qx'), { shown: 'K7M2QX', code: 'K7M2QX' });
  assert.equal(typedCode('K7M2QXX').code, null);
  const account = src('lib/settings/AccountPart.svelte');
  assert.ok(account.includes('<Item name="Allow a device" kind="add" onclick={() => ((typed = \'\'), (allowing = true))} />'));
  assert.ok(account.includes('canSubmit={!!typedNow.code} onsubmit={() => ((pairs.allow = typedNow.code), (allowing = false))}'));
  assert.ok(account.includes('<input value={typedNow.shown} oninput={(e) => (typed = e.currentTarget.value)}'));
});

test('the_allow_sheet_offline', () => {
  // QR07.6: out of reach, the line every page says then, and no Allow; never an Allow queued.
  assert.equal(allowShows(ASK, true), 'away');
  assert.equal(allowShows(null, true), 'away');
  assert.ok(sheet.includes("{#if shows === 'away'}\n      <p class=\"text\">Allowing a device needs Cratarr.</p>"));
  assert.ok(sheet.includes('const shows = $derived(allowShows(read, reach.away));'));
  assert.doesNotMatch(sheet, /later|outbox/, 'nothing kept to send later');
});
