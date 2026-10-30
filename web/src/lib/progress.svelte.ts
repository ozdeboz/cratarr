// What the user is waiting for: requests they caused that have not answered yet (api.ts counts them; background
// polling does not count). lib/ProgressBar.svelte shows it as a thin bar at the top (docs/ui-conventions.md → Feedback).

import { untrack } from 'svelte';

/** `part`: how far known work has come (0 to 1; null: unknown, the bar creeps). */
export const loading = $state({ count: 0, part: null as number | null });

/** Known work, step by step (the owner, 2026-10-25: thousands of albums read, no bar): counted as loading, and the bar
 *  shows how far it is. `run` is given the step to call after each of `total`. */
export async function trackSteps<T>(total: number, run: (step: () => void) => Promise<T>): Promise<T> {
  let done = 0;
  untrack(() => {
    loading.count++;
    loading.part = 0;
  });
  try {
    return await run(() => untrack(() => (loading.part = ++done / Math.max(total, 1))));
  } finally {
    untrack(() => {
      loading.count--;
      loading.part = null;
    });
  }
}

/** Count `work` as loading until it settles. Not a dependency of the caller: a request started in an effect would
 *  otherwise read and write the count there, and run again without end. */
export async function track<T>(work: Promise<T>): Promise<T> {
  untrack(() => loading.count++);
  try {
    return await work;
  } finally {
    untrack(() => loading.count--);
  }
}
