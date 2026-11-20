// The conversion queue (decision 0013): albums waiting for their lossless files above the target library's quality
// limit to be converted, to FLAC, before Keep or Replace.
// Followed app-wide: the Incoming tab shows how many wait, Incoming's rows show their progress, and a finished job
// says so in a toast on whatever page is open. Only jobs this page saw unfinished are told, so a reload never replays
// old ones. The live channel brings every change (`jobs`) to every account; only while it is down does an administrator's
// page poll (the list is theirs to ask for). A user is told only of what their own page queued, with its Undo (W13,
// lib/jobsTold).
import { untrack } from 'svelte';
import { getJobs, type Job } from './api';
import { on, stream } from './events.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';
import { queuedHere, toldOf, type Queued } from './jobsTold';

const ACTIVE_MS = 3_000;
const IDLE_MS = 30_000;

const open = (j: Job) => j.state === 'waiting' || j.state === 'converting';

class Jobs {
  list = $state<Job[]>([]);
  /** Goes up when a job finishes, so Incoming reads its list again. */
  finished = $state(0);
  /** Jobs seen waiting or converting. */
  #seen = new Set<number>();
  #timer: ReturnType<typeof setTimeout> | undefined;
  #running = false;
  /** Whether this page's account administers: it is told of every job, and polls the list while the live channel is down. */
  #admin = true;
  /** What this page queued and has not been told of yet (a user's own conversions). */
  #mine: Queued[] = [];

  /** A move this page asked for waits for its conversion: its end is told here, a user's too. */
  queued(q: Queued) {
    this.#mine = [...this.#mine, q];
  }

  /** Waiting or converting. */
  get open(): Job[] {
    return this.list.filter(open);
  }

  /** A record's job while it waits or converts: of its copy in `library`, as its other copies go on as they are; with
   *  none named, of any copy (a row's meter). */
  of(source: string, album: string, library?: string): Job | undefined {
    return this.list.find((j) => j.source === source && j.album_id === album && open(j) && (!library || !j.from || j.from === library));
  }

  async #refresh() {
    try {
      this.apply(await getJobs());
    } catch {
      /* the next poll tries again */
    }
  }

  /** The whole queue as the server has it now (from a poll, or later from a pushed event). */
  apply(list: Job[]) {
    this.list = list;
    const ended = this.list.filter((j) => !open(j) && this.#seen.has(j.id));
    for (const j of ended) this.#seen.delete(j.id);
    for (const j of this.list.filter(open)) this.#seen.add(j.id);
    if (ended.length) {
      this.finished++;
      const told = toldOf(ended, this.#admin, this.#mine);
      this.#mine = this.#mine.filter((m) => !told.some((j) => queuedHere([m], j)));
      if (told.length) tell(told);
    }
  }

  #schedule() {
    clearTimeout(this.#timer);
    if (this.#running) this.#timer = setTimeout(() => this.#tick(), this.open.length ? ACTIVE_MS : IDLE_MS);
  }

  /** Ask now (after queueing an album, or with the live channel down), then keep following; a user's page hears it on the
   *  live channel. */
  async poke() {
    if (this.#admin) await this.#refresh();
    this.#schedule();
  }

  /** Follow the queue (`admin`: this page's account administers); returns the stop. Untracked, as live.svelte.ts's poll:
   *  started in an effect, its first look at the live channel must not make the channel's opening re-run it. */
  start(admin = true): () => void {
    this.#admin = admin;
    this.#running = true;
    const off = on<Job[]>('jobs', (list) => this.apply(list));
    untrack(() => this.#tick());
    return () => {
      this.#running = false;
      off();
      clearTimeout(this.#timer);
    };
  }

  /** Poll only while the live channel is down, and only an administrator's page: the list is theirs to ask for. */
  async #tick() {
    if (!stream.connected && this.#admin) await this.#refresh();
    clearTimeout(this.#timer);
    if (this.#running) this.#timer = setTimeout(() => this.#tick(), this.open.length ? ACTIVE_MS : IDLE_MS);
  }
}

export const jobs = new Jobs();

/** One toast for what just finished: a failure first (the album stays in Incoming), else what went into Music. */
function tell(ended: Job[]) {
  const failed = ended.find((j) => j.state === 'failed');
  if (failed) {
    toast.show({ text: `Could not ${failed.action} ${failed.title}: ${failed.why ?? 'the conversion failed'}`, tone: 'error' });
    return;
  }
  const done = ended.filter((j) => j.undo);
  if (!done.length) return;
  const verb = done.every((j) => j.action === 'keep') ? 'Added' : done.every((j) => j.action === 'replace') ? 'Replaced' : 'Moved';
  const what = done.length === 1 ? done[0].title : `${done.length} albums`;
  withUndo(
    `${verb} ${what}.`,
    done.map((j) => j.undo),
    { undone: `${what} ${done.length === 1 ? 'is' : 'are'} back where ${done.length === 1 ? 'it was' : 'they were'}.`, after: () => jobs.finished++ },
  );
}
