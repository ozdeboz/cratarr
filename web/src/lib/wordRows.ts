// A record's words under its title (decision 0097, "Shown"): one labelled row per kind it has, Genre, Style, Mood, Tags,
// in that order, each word a chip (a tap its crate, `crate:<kind>:<word>`). Read from the album view's `words`, the
// server's one store; shared/words.json's `expect.words` is what every reader sees (tests/words.test.ts). Pure.
import type { RecordWords, WordKind } from './api';

export const WORD_ROW_LABEL: Record<WordKind, string> = { genre: 'Genre', style: 'Style', mood: 'Mood', tag: 'Tags' };
const ORDER: readonly WordKind[] = ['genre', 'style', 'mood', 'tag'];

export type WordRow = { kind: WordKind; label: string; words: string[] };

/** The record's rows: each kind it has a word of, in the decision's order, its words strongest first. */
export function wordRows(w: RecordWords | null | undefined): WordRow[] {
  return ORDER.flatMap((kind) => {
    const k = w?.kinds.find((x) => x.kind === kind);
    return k?.words.length ? [{ kind, label: WORD_ROW_LABEL[kind], words: k.words.map((x) => x.word) }] : [];
  });
}
