// A part's Reset (the owner, 2026-10-17, after Finamp's settings screens, each with a reset in its bar): a part that
// holds preferences declares them while it shows, and the panel's bar (lib/SettingsPanel) offers Reset while any of
// them is not its default; after a look at the list of what changes ("Theme: Dark → Automatic") it puts those back.
// One declaration for the list and for what Reset does (the owner, 2026-11-01: the sheet's words had drifted from
// what it reset, and Reset showed whether or not anything would change). Never servers, accounts, keys, folders,
// addresses, the sign-in requirement or what was imported: those are not preferences, and their parts offer none.
import { DEFAULTS, prefs, type Prefs } from '../prefs.svelte';

/** One preference: its name as its row says it, what it is now, its default, how to set it, and how to say a value
 *  (none: on and off for a switch, else "As you set it" and "As it came"); `section`: the section of the part it is
 *  in, whose own Reset puts back only those (the owner, 2026-11-02: a section's and the part's Reset disagreed). */
export type ResetItem<T = unknown> = { name: string; now(): T; def: T; set(v: T): unknown; say?(v: T): string; section?: string };

/** A preference kept in `prefs`, its default `DEFAULTS`'. */
export function pref<K extends keyof Prefs>(key: K, name: string, say?: (v: Prefs[K]) => string, section?: string): ResetItem<Prefs[K]> {
  return { name, now: () => prefs[key], def: DEFAULTS[key], set: (v) => (prefs[key] = structuredClone(v)), say, section };
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const sayOf = (i: ResetItem, v: unknown, first: boolean) =>
  i.say ? i.say(v as never) : typeof v === 'boolean' ? (v ? 'On' : 'Off') : v === null || typeof v === 'object' ? (first ? 'As you set it' : 'As it came') : String(v);

// Each part's preferences, by the part's id (the owner, 2026-11-02: the Import part's Reset listed the tape's): kept for
// as long as that part shows, and read for the part shown by its id, never "whichever declared last". (A list kept in
// state is a proxy of itself, so it never matched the list given and was never taken back: the last part's stayed for
// a part that declares none.)
const offered = $state<Record<string, { token: number; items: ResetItem[] }>>({});
let tokens = 0;

/** Declares part `part`'s preferences while it shows; called in a part's script (null: none here now). */
export function offerReset(part: string, items: () => ResetItem[] | null) {
  $effect(() => {
    const list = items();
    if (!list) return;
    const token = ++tokens;
    offered[part] = { token, items: list };
    return () => {
      if (offered[part]?.token === token) delete offered[part];
    };
  });
}

const itemsOf = (part: string): ResetItem[] => offered[part]?.items ?? [];

/** What a Reset of `items` would change (of `section` only, given one): each not at its default, now → default. */
export function rowsOf(items: readonly ResetItem[], section?: string): { name: string; from: string; to: string }[] {
  return items
    .filter((i) => (section === undefined || i.section === section) && !same(i.now(), i.def))
    .map((i) => ({ name: i.name, from: sayOf(i, i.now(), true), to: sayOf(i, i.def, false) }));
}

/** Puts back what of `items` (of `section` only, given one) is not at its default. */
export async function resetOf(items: readonly ResetItem[], section?: string) {
  for (const i of items) if ((section === undefined || i.section === section) && !same(i.now(), i.def)) await i.set(structuredClone(i.def));
}

/** What part `part`'s Reset would change. */
export const resetRows = (part: string) => rowsOf(itemsOf(part));

/** Puts back what of part `part` is not at its default. */
export const runReset = (part: string) => resetOf(itemsOf(part));
