// Types and years into the files (0.13; the owner, 2026-10-31): a switch in Settings › Libraries, off at the start; on,
// Health lists the records whose files differ from their types, year and Classical, and its review writes them
// (decision 0092: always asked, however few). Read where it shows.
import { getFileFacts, setFileFacts, type FileFacts } from './api';
import { me } from './me.svelte';
import { toast } from './toast.svelte';

export const fileFacts = $state({ value: null as FileFacts | null });

/** Read (an administrator's; a server without it: nothing shows). */
export function loadFileFacts() {
  if (!me.admin) return;
  getFileFacts().then(
    (v) => (fileFacts.value = v),
    () => {
      /* a server before it: the row and the line stay away */
    },
  );
}

export async function switchFileFacts(on: boolean) {
  try {
    fileFacts.value = await setFileFacts(on);
  } catch (e) {
    toast.fail(e, "The switch couldn't be changed");
  }
}
