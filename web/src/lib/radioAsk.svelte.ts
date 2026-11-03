// One radio sheet, wherever radio starts (decision 0094 #5): a crate's, a record's, a playlist's, an artist's, a
// selection's or a track's Start radio opens it with what it starts from; lib/RadioSheet asks where (only that list,
// endlessly, or like it), how it moves on, and whole records, then starts it. One sheet, drawn once (App).
import type { Collection } from './api';

/** What radio starts from, said by its kind ("Only this crate"). */
export type RadioKind = 'crate' | 'record' | 'playlist' | 'artist' | 'selection' | 'track';
export type RadioAsk = {
  /** Its name, for the queue sheet. */
  name: string | null;
  kind: RadioKind;
  /** What "Only this …" stays inside (the server's `within`); none where it cannot (a track). */
  within: Collection | null;
  /** Its tracks to be like (the radio's seed), asked for as it starts. */
  seed: () => Promise<string[]>;
};

export const radioSheet = $state<{ ask: RadioAsk | null }>({ ask: null });

/** Radio from `ask`: the sheet opens on the last choices (lib/RadioSheet). */
export const openRadio = (ask: RadioAsk) => (radioSheet.ask = ask);
