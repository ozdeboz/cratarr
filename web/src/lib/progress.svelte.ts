// What the user is waiting for: requests they caused that have not answered yet (api.ts counts them; background
// polling does not count). lib/ProgressBar.svelte shows it as a thin bar at the top (docs/ui-conventions.md → Feedback).

import { untrack } from 'svelte';

export const loading = $state({ count: 0 });

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
