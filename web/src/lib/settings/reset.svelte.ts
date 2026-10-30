// A part's Reset (the owner, 2026-10-17, after Finamp's settings screens, each with a reset in its bar): a part that
// holds preferences offers one while it shows, and the panel's bar (lib/SettingsPanel) shows it; after a confirm it
// puts that part's preferences back to their defaults. Never servers, accounts, keys, folders, addresses, the sign-in
// requirement or what was imported: those are not preferences, and their parts offer none.

export const partReset = $state<{ run: (() => unknown) | null; what: string }>({ run: null, what: '' });

/**
 * Offers this part's Reset while it shows; called in a part's script. `what`: the preferences it puts back, in a line
 * (null: none here now, as a browser's This device part, whose name is not a preference).
 */
export function offerReset(what: () => string | null, run: () => unknown) {
  $effect(() => {
    const line = what();
    if (line === null) return;
    partReset.run = run;
    partReset.what = line;
    return () => {
      if (partReset.run === run) {
        partReset.run = null;
        partReset.what = '';
      }
    };
  });
}
