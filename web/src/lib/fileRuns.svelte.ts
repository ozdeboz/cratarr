// One way to change files (decision 0092; the owner, 2026-11-01): the runs Cratarr makes on the library's files, read
// when "Changes to files" or a review shows and kept up to date from the live channel (`file_run`); the panel's
// addresses for the three views (Health's `changes`: the list; `changes/<kind>[:<album>]`: a plan to review;
// `changes/run:<id>`: a run as written). Administrators' only, as the server's routes are.
import { applyFilePlan, getFileRuns, undoFileRun, type FileRun } from './api';
import { on } from './events.svelte';
import { withRun } from './fileChangeRules';
import { me } from './me.svelte';
import { HEALTH, openStep, viewOver, viewPlaylist } from './panel.svelte';
import { toast } from './toast.svelte';

export const CHANGES = 'changes';

export const fileRuns = $state({ list: null as FileRun[] | null });

let listening = false;
/** The runs of the last month, then each one's news as it comes. */
export function loadRuns() {
  if (!me.admin) return;
  if (!listening) {
    listening = true;
    on<FileRun>('file_run', (s) => (fileRuns.list = withRun(fileRuns.list ?? [], s)));
  }
  getFileRuns().then(
    (l) => (fileRuns.list = l),
    () => (fileRuns.list ??= []),
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
/** A run as written, a step deeper. */
export const openRun = (id: string) => openStep(address(`run:${id}`));

/** The ticked records written: the list then shows the run under way. */
export async function apply(kind: string, albums: string[]): Promise<boolean> {
  try {
    fileRuns.list = withRun(fileRuns.list ?? [], await applyFilePlan(kind, albums));
    openChanges();
    return true;
  } catch (e) {
    toast.fail(e, "The files couldn't be written");
    return false;
  }
}

/** A run undone, or some of its albums. */
export async function undoRun(id: string, albums?: string[]) {
  try {
    fileRuns.list = withRun(fileRuns.list ?? [], await undoFileRun(id, albums));
  } catch (e) {
    toast.fail(e, "It couldn't be undone");
  }
}
