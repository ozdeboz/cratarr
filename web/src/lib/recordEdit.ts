// A record's own values set by hand (0.13; the owner, 2026-10-31: per-record overrides, administrators'): its type, its
// secondary types, its year, its genres and whether it is classical, and (decision 0097, the words' hand layer) its
// styles, moods and tags, one household-wide value each, kept by Cratarr
// beside what it finds (PUT /api/albums/overrides). Pure, so the tests take it as it is: what the Edit sheet
// (lib/RecordEdit) shows for one record or several (a value they share, or mixed; whether it is set by hand), and what
// it sends: only the fields changed, a value to set, null to go back to what Cratarr finds.

export const RECORD_TYPES = ['album', 'ep', 'single', 'broadcast', 'other'] as const;
export type RecordType = (typeof RECORD_TYPES)[number];
export const TYPE_LABEL: Record<RecordType, string> = { album: 'Album', ep: 'EP', single: 'Single', broadcast: 'Broadcast', other: 'Other' };

/** MusicBrainz's secondary types, as Cratarr keeps them (lower case). */
export const SECONDARY_TYPES = [
  'compilation',
  'live',
  'soundtrack',
  'remix',
  'dj-mix',
  'mixtape/street',
  'demo',
  'spokenword',
  'interview',
  'audiobook',
  'audio drama',
  'field recording',
] as const;
const SECONDARY_LABEL: Record<string, string> = { 'dj-mix': 'DJ-mix', 'mixtape/street': 'Mixtape/Street', spokenword: 'Spoken word' };
/** One of the twelve. */
export const isSecondary = (v: string): boolean => (SECONDARY_TYPES as readonly string[]).includes(v);
export const secondaryLabel = (v: string) => SECONDARY_LABEL[v] ?? v.charAt(0).toUpperCase() + v.slice(1);

export type OverrideField = 'type' | 'secondary' | 'year' | 'genres' | 'classical' | 'styles' | 'moods' | 'tags';
export const OVERRIDE_FIELDS: readonly OverrideField[] = ['type', 'secondary', 'year', 'genres', 'classical', 'styles', 'moods', 'tags'];
/** The words' fields (decision 0097), each a kind of the record's words. */
export type WordField = 'genres' | 'styles' | 'moods' | 'tags';
export const WORD_FIELDS: readonly WordField[] = ['genres', 'styles', 'moods', 'tags'];
export const WORD_KIND: Record<WordField, 'genre' | 'style' | 'mood' | 'tag'> = { genres: 'genre', styles: 'style', moods: 'mood', tags: 'tag' };
export const WORD_FIELD_NAME: Record<WordField, string> = { genres: 'Genres', styles: 'Styles', moods: 'Moods', tags: 'Tags' };

/** What a PUT sets: a field present sets it (null: back to what Cratarr finds); one absent is left alone. */
export type OverrideSet = {
  type?: RecordType | null;
  secondary?: string[] | null;
  year?: number | null;
  genres?: string[] | null;
  classical?: boolean | null;
  styles?: string[] | null;
  moods?: string[] | null;
  tags?: string[] | null;
};

/** A record as the sheet reads it: what it is now (found or set), and which of those were set by hand. A word field
 *  null: not read (several records' words are not asked for). */
export type Editable = {
  type: string | null;
  secondary: string[];
  year: number | null;
  genres: string[] | null;
  classical: boolean | null;
  styles: string[] | null;
  moods: string[] | null;
  tags: string[] | null;
  set: readonly OverrideField[];
};

type Value<F extends OverrideField> = Editable[F];

/** A field as several records have it: their one value, or `mixed`; `set`: any of them has it set by hand. */
export type Shared<F extends OverrideField> = { value: Value<F> | null; mixed: boolean; set: boolean };

const same = (a: unknown, b: unknown) =>
  Array.isArray(a) && Array.isArray(b) ? a.length === b.length && [...a].sort().every((x, i) => x === [...b].sort()[i]) : a === b;

export function sharedOf<F extends OverrideField>(records: readonly Editable[], field: F): Shared<F> {
  const set = records.some((r) => r.set.includes(field));
  if (!records.length) return { value: null, mixed: false, set };
  const first = records[0][field];
  return records.every((r) => same(r[field], first)) ? { value: first, mixed: false, set } : { value: null, mixed: true, set };
}

/** The fields to send: those the sheet changed, less what would change nothing (a reset of what none has set; a value
 *  every record has set by hand already). Empty: nothing to save. */
export function toSend(records: readonly Editable[], draft: OverrideSet): OverrideSet {
  const out: OverrideSet = {};
  for (const f of OVERRIDE_FIELDS) {
    if (!(f in draft)) continue;
    const to = draft[f];
    const s = sharedOf(records, f);
    if (to === null || to === undefined) {
      if (s.set) (out as Record<string, unknown>)[f] = null;
      continue;
    }
    const allSet = records.every((r) => r.set.includes(f));
    if (allSet && !s.mixed && same(s.value, to)) continue;
    (out as Record<string, unknown>)[f] = Array.isArray(to) ? [...new Set(to)] : to;
  }
  return out;
}

/** A field as the sheet shows it: the draft's where it changed it (`reset`: back to what Cratarr finds, not known until
 *  it has found it again), else what the records share. */
export function shownOf<F extends OverrideField>(records: readonly Editable[], draft: OverrideSet, field: F) {
  if (field in draft) {
    const to = draft[field];
    return to === null ? { value: null, mixed: false, set: false, reset: true } : { value: to as Value<F>, mixed: false, set: true, reset: false };
  }
  return { ...sharedOf(records, field), reset: false };
}

/** The classical choice (a slim track of three: as Cratarr finds it, yes, no; '' where several differ). */
export type ClassicalChoice = 'auto' | 'yes' | 'no' | '';
export function classicalChoiceOf(records: readonly Editable[], draft: OverrideSet): ClassicalChoice {
  if ('classical' in draft) return draft.classical === null ? 'auto' : draft.classical ? 'yes' : 'no';
  const marked = records.filter((r) => r.set.includes('classical')).length;
  if (marked === 0) return 'auto';
  const s = sharedOf(records, 'classical');
  return marked < records.length || s.mixed ? '' : s.value ? 'yes' : 'no';
}
export const classicalOfChoice = (c: ClassicalChoice): boolean | null => (c === 'yes' ? true : c === 'no' ? false : null);

/** A record of the Library's list as the sheet reads it (its words given where read). A classical mark from before
 *  the overrides counts as set by hand. */
export function editableOf(
  a: { type?: string | null; secondary?: string[]; year: number | null; classical?: boolean; classical_mark?: string | null; set?: readonly string[] },
  words: Partial<Record<WordField, string[]>> | null = null,
): Editable {
  const set = OVERRIDE_FIELDS.filter((f) => a.set?.includes(f) || (f === 'classical' && !!a.classical_mark));
  const w = (f: WordField) => (words ? (words[f] ?? []) : null);
  return { type: a.type ?? null, secondary: a.secondary ?? [], year: a.year, genres: w('genres'), classical: a.classical ?? null, styles: w('styles'), moods: w('moods'), tags: w('tags'), set };
}

/** One record's words as its view has them (decision 0097's one store, the album view's `words`), per field; its
 *  genres from `genres` where the view has no words. */
export function wordsOfView(v: { genres: string[]; words?: { kinds: { kind: string; words: { word: string }[] }[] } }): Record<WordField, string[]> {
  const of = (k: string) => v.words?.kinds.find((x) => x.kind === k)?.words.map((x) => x.word) ?? [];
  return { genres: v.words ? of('genre') : v.genres, styles: of('style'), moods: of('mood'), tags: of('tag') };
}

/** A year typed: a whole year from 1000 to 2999, else none. */
export function yearOf(typed: string): number | null {
  const n = Number(typed.trim());
  return Number.isInteger(n) && n >= 1000 && n <= 2999 ? n : null;
}
