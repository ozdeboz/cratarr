// Each library's own tag splitting (GEN-DESIGN §4, OQ-GEN-22; the owner, 2026-11-20, "Per library in 0.18"; GEN.63): an
// advanced item on Libraries › <library> beside Use and Quality limit, the top level's item the default for new libraries and
// own music, the same editors for both, and "&" refused as a genre separator with the reason. These read the source where
// a page needs a whole app to run.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const { AMPERSAND, encodingOf, genreSeparatorsRefused, splittingLine, textOf } = await import('../src/lib/settings/tagRules.ts');

const source = (file: string) => readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');

test('a library has Tag splitting of its own beside Use and Quality limit, on a page of its own', () => {
  const page = source('lib/settings/LibrariesPart.svelte');
  const library = page.slice(page.indexOf('{#if open}'), page.indexOf('{:else if wordsThing && thing}'));
  const [use, quality, tags, colour] = ['name="Use"', 'name="Quality limit"', 'name="Tag splitting"', 'name="Dot colour"'].map((n) => library.indexOf(n));
  assert.ok(use > 0 && use < quality && quality < tags && tags < colour, 'beside Use and Quality limit, a used library\'s');
  assert.ok(library.includes('<Item name="Tag splitting" line={splittingLine(libText(l))} onclick={() => openThing(\'libraries\', libTagsThing(l))} />'));
  assert.ok(page.includes('const libTagsThing = (l: Lib) => `tags:${l.d.id}:${l.id}`;'));
  assert.ok(page.includes("const tagsOf = $derived(thing?.startsWith('tags:') ? libNamed(thing.slice(5)) : null);"));
  // Its page: the library's own encoding (the default it starts from until saved), set as the library's.
  assert.ok(page.includes('const libText = (l: Lib): SplitText => textOf(ed.policyOf(l.d, l.id).tag_rules ?? ed.rules);'));
  assert.ok(page.includes('ed.setPolicy(l.d, l.id, { tag_rules: encodingOf({ ...libText(l), [key]: v }) })'));
  assert.ok(page.includes("{:else if tagsOf}\n  {@const l = tagsOf}\n  {@render splitting(libText(l), (key, v) => setLibText(l, key, v))}"));
  assert.ok(page.includes("? 'For new libraries and own music'\n      : tagsOf\n        ? 'Tag splitting'"), 'named as its row');
  // A library gone, its page goes too.
  assert.ok(page.includes("if (thing && thing !== 'tags' && !wordsThing && !open && !tagsOf) backFromThing('libraries');"));
});

test('the top level\'s Tag splitting is the default for new libraries and own music, with the same editors; the aliases stay the install\'s', () => {
  const page = source('lib/settings/LibrariesPart.svelte');
  assert.ok(page.includes(`<Item name="For new libraries and own music" line={splittingLine(ed.ruleText)} onclick={() => openThing('libraries', 'tags')} />`));
  assert.ok(!page.includes('name="Tag splitting"\n      line={[`Artists'), 'no top-level item of the one set');
  const defaults = page.slice(page.indexOf("{:else if thing === 'tags'}"), page.indexOf('{:else}', page.indexOf("{:else if thing === 'tags'}")));
  assert.ok(defaults.includes('{@render splitting(ed.ruleText, (key, v) => (ed.ruleText[key] = v))}'));
  // The names given otherwise are the install's, edited once, here, never per library.
  assert.ok(defaults.includes('<Group title="Named otherwise">'));
  const own = page.slice(page.indexOf('{:else if tagsOf}'), page.indexOf("{:else if thing === 'tags'}"));
  assert.ok(!own.includes('Named otherwise') && !own.includes('Aliases'));
  // One set of editors for both: separators, then what is never split, each through the value sheet.
  const editors = page.slice(page.indexOf('{#snippet splitting('), page.indexOf('{/snippet}', page.indexOf('{#snippet splitting(')));
  for (const name of ['Artist separators', 'Genre separators', 'name="Artists"', 'name="Genres"']) assert.ok(editors.includes(name), name);
  assert.equal(page.split('{@render splitting(').length - 1, 2, 'a library\'s and the default\'s');
  assert.ok(!page.includes('<Item\n      name="Artist separators"\n      line={ed.ruleText'), 'no second copy of the editors');
});

test('"&" is refused as a genre separator, with the reason, in both editors (OQ-GEN-21)', () => {
  assert.equal(genreSeparatorsRefused('/ ; , &'), AMPERSAND);
  assert.equal(genreSeparatorsRefused('&'), AMPERSAND);
  assert.equal(genreSeparatorsRefused('; &;'), AMPERSAND);
  assert.equal(genreSeparatorsRefused('/ ; ,'), null);
  assert.equal(genreSeparatorsRefused(''), null);
  assert.match(AMPERSAND, /Drum & Bass and R&B/);
  const page = source('lib/settings/LibrariesPart.svelte');
  const editors = page.slice(page.indexOf('{#snippet splitting('), page.indexOf('{/snippet}', page.indexOf('{#snippet splitting(')));
  // The genre separators' sheet refuses; the artists' keep their own rules ("&" is part of credits, never split anyway).
  assert.ok(editors.includes("refuse: genreSeparatorsRefused, save: (v) => set('genreSeps', v)"));
  assert.equal(editors.split('refuse:').length - 1, 1);
  // Refused: said under the field as it is typed and at Done, the sheet stays (its save throws), nothing taken.
  const sheet = page.slice(page.indexOf('{#if typing}'), page.indexOf('{/if}', page.indexOf('{/snippet}', page.indexOf('{#if typing}'))));
  assert.ok(sheet.includes('oninput={t.refuse ? (v) => (refused = t.refuse?.(v) ?? null) : null}'));
  assert.match(sheet, /refused = t\.refuse\?\.\(v\) \?\? null;\n\s+if \(refused\) throw new Error\(refused\);\n\s+t\.save\(v\);/);
  assert.ok(sheet.includes('{#if refused}<p class="check bad">{refused}</p>{/if}'));
  // The server says it the same way when it refuses one.
  const server = readFileSync(new URL('../../server/crates/store/src/catalogue/normalise.rs', import.meta.url), 'utf8');
  assert.ok(server.includes(`pub const AMPERSAND: &str = "${AMPERSAND.replace(/"/g, '\\"')}";`));
});

test('an encoding goes to its editors as text and back in the order the server keeps it', () => {
  const e = { artist_separators: ['/', ';'], genre_separators: ['/', ';', ','], artist_keep: ['AC/DC', '30/70'], genre_keep: ['Funk / Soul'] };
  const t = textOf(e);
  assert.deepEqual(t, { artistSeps: '/ ;', genreSeps: '/ ; ,', artistKeep: 'AC/DC\n30/70', genreKeep: 'Funk / Soul' });
  assert.equal(JSON.stringify(encodingOf(t)), JSON.stringify(e), 'compared with the saved by its JSON');
  assert.deepEqual(encodingOf({ artistSeps: '  ', genreSeps: ' ; ', artistKeep: '\n AC/DC \n\n', genreKeep: '' }), { artist_separators: [], genre_separators: [';'], artist_keep: ['AC/DC'], genre_keep: [] });
  assert.equal(splittingLine(t), 'Artists: / ; · Genres: / ; ,');
  assert.equal(splittingLine(textOf({ artist_separators: [], genre_separators: [], artist_keep: [], genre_keep: [] })), 'Artists: not split · Genres: not split');
});
