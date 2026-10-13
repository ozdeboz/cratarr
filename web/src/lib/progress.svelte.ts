// What the user is waiting for: requests they caused that have not answered yet (api.ts counts them; background
// polling does not count). lib/ProgressBar.svelte shows it as a thin bar at the top (docs/ui-conventions.md → Feedback).

export const loading = $state({ count: 0 });

/** Count `work` as loading until it settles. */
export async function track<T>(work: Promise<T>): Promise<T> {
  loading.count++;
  try {
    return await work;
  } finally {
    loading.count--;
  }
}
