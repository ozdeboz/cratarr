// The conversion queue (decision 0013): albums waiting for their hi-res files to be converted before Keep or Replace.
// Followed app-wide: the Incoming tab shows how many wait, Incoming's rows show their progress, and a finished job
// says so in a toast on whatever page is open. Only jobs this page saw unfinished are told, so a reload never replays
// old ones. The live channel brings every change (`jobs`); only while it is down does this poll.
import { untrack } from 'svelte';
import { getJobs, type Job } from './api';
import { on, stream } from './events.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

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

  /** Waiting or converting. */
  get open(): Job[] {
    return this.list.filter(open);
  }

  /** The album's job while it waits or converts. */
  of(source: string, album: string): Job | undefined {
    return this.list.find((j) => j.source === source && j.album_id === album && open(j));
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
      tell(ended);
    }
  }

  #schedule() {
    clearTimeout(this.#timer);
    if (this.#running) this.#timer = setTimeout(() => this.#tick(), this.open.length ? ACTIVE_MS : IDLE_MS);
  }

  /** Ask now (after queueing an album, or with the live channel down), then keep following. */
  async poke() {
    await this.#refresh();
    this.#schedule();
  }

  /** Follow the queue; returns the stop. Untracked, as live.svelte.ts's poll: started in an effect, its first look at
   *  the live channel must not make the channel's opening re-run it. */
  start(): () => void {
    this.#running = true;
    const off = on<Job[]>('jobs', (list) => this.apply(list));
    untrack(() => this.#tick());
    return () => {
      this.#running = false;
      off();
      clearTimeout(this.#timer);
    };
  }

  /** Poll only while the live channel is down. */
  async #tick() {
    if (!stream.connected) await this.#refresh();
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
  const verb = done.every((j) => j.action === 'keep') ? 'Kept' : done.every((j) => j.action === 'replace') ? 'Replaced' : 'Moved';
  const what = done.length === 1 ? done[0].title : `${done.length} albums`;
  withUndo(
    `${verb} ${what} ${verb === 'Moved' ? 'into' : 'in'} Music.`,
    done.map((j) => j.undo),
    { undone: `${what} ${done.length === 1 ? 'is' : 'are'} back in Incoming.`, after: () => jobs.finished++ },
  );
}
