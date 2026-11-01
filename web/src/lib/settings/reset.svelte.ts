// A part's Reset (the owner, 2026-10-17, after Finamp's settings screens, each with a reset in its bar): a part that
// holds preferences declares them while it shows, and the panel's bar (lib/SettingsPanel) offers Reset while any of
// them is not its default; after a look at the list of what changes ("Theme: Dark → Automatic") it puts those back.
// One declaration for the list and for what Reset does (the owner, 2026-11-01: the sheet's words had drifted from
// what it reset, and Reset showed whether or not anything would change). Never servers, accounts, keys, folders,
// addresses, the sign-in requirement or what was imported: those are not preferences, and their parts offer none.
import { DEFAULTS, prefs, type Prefs } from '../prefs.svelte';

/** One preference: its name as its row says it, what it is now, its default, how to set it, and how to say a value
 *  (none: on and off for a switch, else "As you set it" and "As it came"). */
export type ResetItem<T = unknown> = { name: string; now(): T; def: T; set(v: T): unknown; say?(v: T): string };

/** A preference kept in `prefs`, its default `DEFAULTS`'. */
export function pref<K extends keyof Prefs>(key: K, name: string, say?: (v: Prefs[K]) => string): ResetItem<Prefs[K]> {
  return { name, now: () => prefs[key], def: DEFAULTS[key], set: (v) => (prefs[key] = structuredClone(v)), say };
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const sayOf = (i: ResetItem, v: unknown, first: boolean) =>
  i.say ? i.say(v as never) : typeof v === 'boolean' ? (v ? 'On' : 'Off') : v === null || typeof v === 'object' ? (first ? 'As you set it' : 'As it came') : String(v);

export const partReset = $state<{ items: ResetItem[] }>({ items: [] });

/** Declares this part's preferences while it shows; called in a part's script (null: none here now). */
export function offerReset(items: () => ResetItem[] | null) {
  $effect(() => {
    const list = items();
    if (!list) return;
    partReset.items = list as ResetItem[];
    return () => {
      if (partReset.items === list) partReset.items = [];
    };
  });
}

/** What Reset would change: each preference not at its default, now → default. */
export function resetRows(): { name: string; from: string; to: string }[] {
  return partReset.items.filter((i) => !same(i.now(), i.def)).map((i) => ({ name: i.name, from: sayOf(i, i.now(), true), to: sayOf(i, i.def, false) }));
}

/** Puts back what is not at its default. */
export async function runReset() {
  for (const i of partReset.items) if (!same(i.now(), i.def)) await i.set(structuredClone(i.def));
}
