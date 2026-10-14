// Export's options (decision 0007, the panel's Export tab, decision 0025): kept in this browser, so they stay as they
// were set while the selection changes, across pages and a reload.

type Options = {
  tracks: boolean;
  flat: boolean;
  combine: boolean;
  /** The combined playlist's name; it follows the selection until typed in (`named`). */
  name: string;
  named: boolean;
};
const KEY = 'cratarr-export';
const blank = (): Options => ({ tracks: false, flat: false, combine: false, name: '', named: false });

function kept(): Options {
  try {
    const k = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (k && typeof k === 'object') return { ...blank(), ...k };
  } catch {
    /* nothing kept */
  }
  return blank();
}

export const exporting = $state<Options>(kept());
$effect.root(() => {
  $effect(() => {
    const s = JSON.stringify(exporting);
    try {
      localStorage.setItem(KEY, s);
    } catch {
      /* not kept */
    }
  });
});
