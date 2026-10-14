// The app's one toast (docs/ui-conventions.md → Feedback): what an action did, or why it failed. Any page or component
// calls `toast.show` or `toast.fail`; App.svelte renders it (lib/Toasts.svelte), outside the page layout. An action's
// toast with Undo goes through lib/undo.svelte.ts (decision 0026).
import { ApiError } from './api';

/** `act`: its one button (Undo, Redo): tapping it closes the toast and runs it. */
export type Toast = { text: string; tone?: 'done' | 'warn' | 'error'; act?: { label: string; run: () => void } };

class Toasts {
  /** The one shown; a new one replaces it. `id` restarts its timer, also when the same text comes again. */
  current = $state<(Toast & { id: number }) | null>(null);
  #seq = 0;

  show(t: Toast) {
    this.current = { ...t, id: ++this.#seq };
  }

  /** An action failed: the server's own words when it gave some, else `fallback` (never a bare status code). */
  fail(e: unknown, fallback: string) {
    this.show({ text: e instanceof ApiError && !e.generic ? e.message : fallback, tone: 'error' });
  }

  dismiss() {
    this.current = null;
  }
}

export const toast = new Toasts();
