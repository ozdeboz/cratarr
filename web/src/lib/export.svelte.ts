// Export's choices (decisions 0007 and 0058: the panel's Export tab, every menu's Download… step): kept in this browser,
// so they stay as they were set while the selection changes, across pages and a reload. Each account's (decision 0101):
// the name follows what was selected, a person's playlists, or what they typed.
import { perAccount, whenAccount } from './account';
import { keptExport, type ExportOptions } from './exportChoices';

const KEY = 'cratarr-export';

function kept(): ExportOptions {
  try {
    return keptExport(JSON.parse(localStorage.getItem(perAccount(KEY)) ?? 'null'));
  } catch {
    return keptExport(null);
  }
}

export const exporting = $state<ExportOptions>(kept());

/** Whose they are: saved under them, even while another's come in. */
let keptOf = perAccount(KEY);
function save() {
  const s = JSON.stringify(exporting);
  try {
    localStorage.setItem(keptOf, s);
  } catch {
    /* not kept */
  }
}

// Another account signed in, or nobody: the one leaving's kept, the one come's taken up.
whenAccount.add(() => {
  if (perAccount(KEY) === keptOf) return;
  save();
  keptOf = perAccount(KEY);
  Object.assign(exporting, kept());
});

$effect.root(() => {
  $effect(save);
});
