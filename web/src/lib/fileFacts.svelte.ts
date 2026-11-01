// Types and years into the files (0.13; the owner, 2026-10-31): a switch in Settings › Libraries, off at the start; on,
// Cratarr writes each record's types, year and Classical into its files' tags, a small change by itself, and Health
// says when many differ, with Write. Read where it shows, and again every two seconds while a run writes.
import { getFileFacts, setFileFacts, writeFileFacts, type FileFacts } from './api';
import { me } from './me.svelte';
import { toast } from './toast.svelte';

export const fileFacts = $state({ value: null as FileFacts | null });

let timer = 0;
function took(v: FileFacts) {
  fileFacts.value = v;
  clearTimeout(timer);
  if (v.writing) timer = window.setTimeout(loadFileFacts, 2000);
}

/** Read (an administrator's; a server without it: nothing shows). */
export function loadFileFacts() {
  if (!me.admin) return;
  getFileFacts().then(took, () => {
    /* a server before it: the row and the line stay away */
  });
}

export async function switchFileFacts(on: boolean) {
  try {
    took(await setFileFacts(on));
  } catch (e) {
    toast.fail(e, "The switch couldn't be changed");
  }
}

export async function writeFacts() {
  try {
    took(await writeFileFacts());
  } catch (e) {
    toast.fail(e, "The files couldn't be written");
  }
}

/** Health's line, while the switch is on: the files that differ (Write), or the run writing them. */
export function factsLine(): { name: string; progress: number | null; write: boolean } | null {
  const v = fileFacts.value;
  if (!v?.on) return null;
  if (v.writing) return { name: `Writing ${v.writing.done.toLocaleString()} of ${v.writing.of.toLocaleString()}`, progress: v.writing.of ? v.writing.done / v.writing.of : 0, write: false };
  if (!v.differ) return null;
  return { name: `${v.differ.toLocaleString()} ${v.differ === 1 ? "record's files differ" : "records' files differ"} from their types and years`, progress: null, write: true };
}
