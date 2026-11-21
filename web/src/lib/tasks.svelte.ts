// The server's long work (`app::tasks`, the owner 2026-10-24: one way to show progress): the signed-in account's tasks
// by kind, as the live channel tells them (`task`); while it is down and one runs, asked every two seconds. A page
// shows its kind's task (`progressOf` for its line and bar) and its result when it ends, also after it was left and
// opened again: the server keeps a finished task an hour.
import { untrack } from 'svelte';
import { getTasks, type Task } from './api';
import { on, stream } from './events.svelte';

const POLL_MS = 2_000;

/** Asked this many times in a row without the server naming a followed job: it is gone (a restart cut it off). */
const MISSED = 3;

class Tasks {
  byKind = $state<Record<string, Task>>({});
  #timer: ReturnType<typeof setTimeout> | undefined;
  #users = 0;
  #off: (() => void) | null = null;
  /** The jobs followed by their id until they end (`ended`), with how often the server's list lacked each. */
  #waiting = new Map<number, { done: (t: Task) => void; fail: (e: Error) => void; seen?: (t: Task) => void; missed: number }>();

  /** The latest of a kind, running or ended. */
  of(kind: string): Task | null {
    return this.byKind[kind] ?? null;
  }

  running(kind: string): boolean {
    const t = this.of(kind);
    return !!t && t.finished === null;
  }

  apply(t: Task) {
    const w = this.#waiting.get(t.id);
    w?.seen?.(t);
    if (t.finished !== null) w?.done(t);
    const had = this.byKind[t.kind];
    // A late word of an older task never hides a newer one.
    if (had && had.id > t.id) return;
    this.byKind = { ...this.byKind, [t.kind]: t };
    this.#schedule();
  }

  /** A task just begun by this page. */
  begun(t: Task) {
    this.apply(t);
  }

  /** Asked now: one the server began on its own may have come after the last asking (Discover's first look). */
  ask() {
    return this.#ask();
  }

  /** A job an open began or joined, followed by its id until it ends (SCENARIOS-LOOKUPS: an open answers 202 with its
   *  task; several of one kind may run at once, one per key). Its end as the live channel tells it, or as asked every two
   *  seconds while that is down; the runner's own words where the server no longer knows it. `seen` is told each word of it
   *  while it runs (what it has found so far, in its `result`). */
  ended(t: Task, seen?: (t: Task) => void): Promise<Task> {
    if (t.finished !== null) return Promise.resolve(t);
    return new Promise((resolve, reject) => {
      const stop = this.follow();
      const end = (then: () => void) => {
        this.#waiting.delete(t.id);
        stop();
        then();
      };
      this.#waiting.set(t.id, { done: (x) => end(() => resolve(x)), fail: (e) => end(() => reject(e)), seen, missed: 0 });
      this.apply(t);
    });
  }

  async #ask() {
    try {
      const listed = (await getTasks()).tasks;
      for (const t of listed) this.apply(t);
      for (const [id, w] of this.#waiting) {
        w.missed = listed.some((t) => t.id === id) ? 0 : w.missed + 1;
        if (w.missed >= MISSED) w.fail(new Error('It stopped before it was done.'));
      }
    } catch {
      /* the next look tries again */
    }
  }

  #schedule() {
    clearTimeout(this.#timer);
    const open = this.#waiting.size > 0 || Object.values(this.byKind).some((t) => t.finished === null);
    if (this.#users && open && !stream.connected) this.#timer = setTimeout(() => this.#ask().then(() => this.#schedule()), POLL_MS);
  }

  /** Follow while a page shows tasks; returns the stop. */
  follow(): () => void {
    this.#users++;
    if (this.#users === 1) {
      this.#off = on<Task>('task', (t) => this.apply(t));
      untrack(() => this.#ask());
    }
    return () => {
      this.#users--;
      if (this.#users === 0) {
        this.#off?.();
        this.#off = null;
        clearTimeout(this.#timer);
      }
    };
  }
}

export const tasks = new Tasks();

const n = (x: number) => x.toLocaleString();

/** A running task's line and bar (ui-conventions, Progress): "Reading scrobbles · 12,400 of 79,000" and how far, or
 *  "Matching" with no bar where nothing is counted. */
export function progressOf(t: Task | null): { line: string; value: number | null } {
  const s = t?.step;
  if (!s) return { line: 'Starting', value: null };
  if (s.total) return { line: `${s.name} · ${n(s.done)} of ${n(s.total)}`, value: s.done / s.total };
  return { line: s.done ? `${s.name} · ${n(s.done)}` : s.name, value: null };
}
