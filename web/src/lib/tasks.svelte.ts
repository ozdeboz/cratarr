// The server's long work (`app::tasks`, the owner 2026-10-24: one way to show progress): the signed-in account's tasks
// by kind, as the live channel tells them (`task`); while it is down and one runs, asked every two seconds. A page
// shows its kind's task (`progressOf` for its line and bar) and its result when it ends, also after it was left and
// opened again: the server keeps a finished task an hour.
import { untrack } from 'svelte';
import { getTasks, type Task } from './api';
import { on, stream } from './events.svelte';

const POLL_MS = 2_000;

class Tasks {
  byKind = $state<Record<string, Task>>({});
  #timer: ReturnType<typeof setTimeout> | undefined;
  #users = 0;
  #off: (() => void) | null = null;

  /** The latest of a kind, running or ended. */
  of(kind: string): Task | null {
    return this.byKind[kind] ?? null;
  }

  running(kind: string): boolean {
    const t = this.of(kind);
    return !!t && t.finished === null;
  }

  apply(t: Task) {
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

  async #ask() {
    try {
      for (const t of (await getTasks()).tasks) this.apply(t);
    } catch {
      /* the next look tries again */
    }
  }

  #schedule() {
    clearTimeout(this.#timer);
    const open = Object.values(this.byKind).some((t) => t.finished === null);
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
