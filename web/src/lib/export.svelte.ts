// Export's choices (decisions 0007 and 0058: the panel's Export tab, every menu's Download… step): kept in this browser,
// so they stay as they were set while the selection changes, across pages and a reload.
import { keptExport, type ExportOptions } from './exportChoices';

const KEY = 'cratarr-export';

function kept(): ExportOptions {
  try {
    return keptExport(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return keptExport(null);
  }
}

export const exporting = $state<ExportOptions>(kept());
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
