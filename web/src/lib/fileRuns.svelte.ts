// One way to change files (decision 0092; the owner, 2026-11-01): the runs Cratarr makes on the library's files, read
// when "Changes to files" or a review shows and kept up to date from the live channel (`file_run`); the panel's
// addresses for the three views (Health's `changes`: the list; `changes/<kind>[:<album>]`: a plan to review;
// `changes/run:<id>`: a run as written). Administrators' only, as the server's routes are.
import { applyFilePlan, getFileRuns, undoFileRun, type FileRecord, type FileRun } from './api';
import { on } from './events.svelte';
import { withRun } from './fileChangeRules';
import { me } from './me.svelte';
import { HEALTH, openStep, viewInPlace, viewOver, viewPlaylist } from './panel.svelte';
import { toast } from './toast.svelte';

export const CHANGES = 'changes';

/** `error`: the runs could not be read, said in place of the list (the owner, 2026-11-02: errors are never shown as
 *  nothing, so never "No changes to files this month." after a failed read). */
export const fileRuns = $state({ list: null as FileRun[] | null, error: null as string | null });

let listening = false;
/** The runs of the last month, then each one's news as it comes. */
export function loadRuns() {
  if (!me.admin) return;
  if (!listening) {
    listening = true;
    on<FileRun>('file_run', (s) => (fileRuns.list = withRun(fileRuns.list ?? [], s)));
  }
  getFileRuns().then(
    (l) => ((fileRuns.list = l), (fileRuns.error = null)),
    (e) => (fileRuns.error = e instanceof Error ? e.message : 'Unexpected error'),
  );
}

/** What the panel shows of Changes to files: the list, a plan of `kind` (for one record), or a run. */
export type ChangesView = { view: 'list' } | { view: 'plan'; kind: string; album: string | null } | { view: 'run'; id: string };
export function changesView(thing: string | null): ChangesView {
  if (!thing) return { view: 'list' };
  if (thing.startsWith('run:')) return { view: 'run', id: thing.slice(4) };
  const [kind, album] = thing.split(':');
  return { view: 'plan', kind, album: album || null };
}
const address = (thing?: string) => `${HEALTH}${CHANGES}${thing ? `/${encodeURIComponent(thing)}` : ''}`;
/** The list, in the panel's place. */
export const openChanges = () => viewPlaylist(address());
/** A plan to review (one record's, from its state line, over what the panel showed). */
export const openReview = (kind: string, album?: string) => (album ? viewOver(address(`${kind}:${album}`)) : viewPlaylist(address(kind)));
/** A run, under way or as written, a step deeper (from the list). */
export const openRun = (id: string) => openStep(address(`run:${id}`));

/** What the sheet sent, for the run's page while it is written (the server keeps a run's records once it is done). */
export const sent = $state({ id: '', what: '', at: 0, records: [] as FileRecord[] });

/** The ticked records written, each as the sheet showed it (its stamp: one changed since is set aside). The sheet then
 *  becomes the run's page where it is, no layer moved (the owner, 2026-11-01: Write took you two layers into Health's
 *  list, and the progress was lost on coming back; "No u turn"): the same page the list opens a step down. */
export async function apply(kind: string, records: FileRecord[]): Promise<boolean> {
  try {
    const s = await applyFilePlan(kind, records);
    fileRuns.list = withRun(fileRuns.list ?? [], s);
    Object.assign(sent, { id: s.id, what: s.what, at: Math.floor(Date.now() / 1000) - 5, records });
    viewInPlace(address(`run:${s.id}`));
    return true;
  } catch (e) {
    toast.fail(e, "The files couldn't be written");
    return false;
  }
}

/** A live run (`l…`) once kept: its kept status (`r…`), the same writer's, kept since it was sent. */
export function keptOf(live: string): FileRun | null {
  if (!live.startsWith('l') || sent.id !== live) return null;
  return fileRuns.list?.find((r) => r.id.startsWith('r') && r.what === sent.what && (r.at ?? 0) >= sent.at) ?? null;
}
/** The run's page for its kept id, in place of the live one's (no step: back still goes where the sheet came from). */
export const keptRun = (id: string) => viewInPlace(address(`run:${id}`));

/** A run undone, or some of its albums. */
export async function undoRun(id: string, albums?: string[]) {
  try {
    fileRuns.list = withRun(fileRuns.list ?? [], await undoFileRun(id, albums));
  } catch (e) {
    toast.fail(e, "It couldn't be undone");
  }
}
