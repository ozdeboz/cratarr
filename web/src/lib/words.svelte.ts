// The Catalogue's words (decision 0097, the controls; administrators'): what Server libraries › Catalogue shows of them,
// read where it shows. Its "Written to files" sheet (the owner, 2026-11-02: a sheet, "keeps it nicely leveled") and its
// "Sources and spellings" pages (2026-11-03: a row, a page per kind) both read and change this one copy. Type and year
// share one write with 0.13's switch, which Health reads too (lib/fileFacts).
import { getWordsCatalogue, mergeWord, setWordAliases, setWordsKind, type CatalogueKind, type WordAliases, type WordKind, type WordSource, type WordsCatalogue, type WordsOfKind } from './api';
import { WORD_ROW_LABEL } from './wordRows';
import { loadFileFacts } from './fileFacts.svelte';
import { toast } from './toast.svelte';

export const KIND_NAMES: Record<CatalogueKind, string> = { type: 'Type', year: 'Year', ...WORD_ROW_LABEL };
/** The kinds with sources and spellings of their own (type and year have neither). */
export const SPELLED: WordKind[] = ['genre', 'style', 'mood', 'tag'];

/** A kind's field in a record's hand words (lib/recordEdit's overrides). */
export const WORD_FIELD_OF: Record<WordKind, 'genres' | 'styles' | 'moods' | 'tags'> = { genre: 'genres', style: 'styles', mood: 'moods', tag: 'tags' };
/** The sources' names (the store's ids; Catalogue's own names where it was read). */
const SOURCE_NAME: Record<WordSource, string> = {
  hand: 'set by hand',
  files: 'files',
  audiomuse: 'AudioMuse',
  household: 'household',
  musicbrainz: 'MusicBrainz',
  discogs: 'Discogs',
  lastfm: 'Last.fm',
};
/** Who says a word, strongest first: "set by hand, Discogs". */
export const sourceNames = (ids: readonly WordSource[]) => ids.map((id) => SOURCE_NAME[id] ?? id).join(', ');

export const words = $state({ value: null as WordsCatalogue | null, error: null as string | null });

export function readWords() {
  getWordsCatalogue().then(
    (c) => ((words.value = c), (words.error = null)),
    (e) => (words.error = e instanceof Error ? e.message : 'Unexpected error'),
  );
}

export const kindOf = (k: CatalogueKind) => words.value?.kinds.find((x) => x.kind === k) ?? null;

function took(now: WordsOfKind) {
  if (!words.value) return;
  // Type and year come back together when either is set: the other as its write.
  const shared = now.kind === 'type' || now.kind === 'year';
  words.value = {
    kinds: words.value.kinds.map((x) => (x.kind === now.kind ? now : shared && (x.kind === 'type' || x.kind === 'year') ? { ...x, write: now.write } : x)),
  };
}
function tookAliases(k: CatalogueKind, aliases: WordAliases[]) {
  if (words.value) words.value = { kinds: words.value.kinds.map((x) => (x.kind === k ? { ...x, aliases } : x)) };
}

export async function setWrite(k: CatalogueKind, write: boolean) {
  try {
    took(await setWordsKind(k, { write }));
    if (k === 'type' || k === 'year') loadFileFacts();
  } catch (e) {
    toast.fail(e, "The switch couldn't be changed");
  }
}

export async function setSource(k: WordKind, id: WordSource, on: boolean) {
  try {
    took(await setWordsKind(k, { sources: { [id]: on } }));
  } catch (e) {
    toast.fail(e, "The switch couldn't be changed");
  }
}

/** A word's other spellings, one a line (none: its entry goes). */
export async function setSpellings(k: WordKind, word: string, text: string) {
  const aliases = [...new Set(text.split('\n').map((s) => s.trim()).filter((s) => s && s !== word))];
  try {
    tookAliases(k, await setWordAliases(k, word, aliases));
  } catch (e) {
    toast.fail(e, "The spellings weren't kept");
  }
}

/** `from` read as `into` everywhere: Merge into…, and Rename everywhere with a new spelling as `into`. */
export async function mergeInto(k: WordKind, from: string, into: string) {
  try {
    tookAliases(k, await mergeWord(k, from, into));
    return true;
  } catch (e) {
    toast.fail(e, "The words weren't merged");
    return false;
  }
}
