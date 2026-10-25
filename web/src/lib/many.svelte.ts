// A change to many at once is asked once more first (the owner, 2026-10-25: 1,069 album hearts from one tap on the
// select bar's heart with every album ticked): more than MANY items, a sheet naming how many (lib/ManyConfirm, App's),
// the act and Cancel. Any change that writes (hearts, playlists, BPM, answering requests); playing does not ask.
import { trackSteps } from './progress.svelte';
import type { IconName } from './Icon.svelte';

/** Up to this many go at once; more ask first (the line the import draws for taking stars away, serve/import). */
export const MANY = 20;

/** The change waiting for its yes (lib/ManyConfirm shows it). */
export const manyAsk = $state({ value: null as { title: string; label: string; icon: IconName; go: () => void } | null });

/** `go` at once for MANY or fewer; else once the sheet's act is tapped (Cancel: never). */
export function askIfMany(count: number, ask: { title: string; label: string; icon: IconName }, go: () => void) {
  if (count <= MANY) return void go();
  manyAsk.value = { ...ask, go };
}

/** One call per item, a few at a time (thousands at once waited in the browser's line until they failed), the bar at
 *  the top saying how far it is (lib/progress). The items whose call failed. */
export async function fewAtATime<T>(items: T[], call: (x: T) => Promise<unknown>, at = 6): Promise<T[]> {
  const failed: T[] = [];
  let next = 0;
  await trackSteps(items.length, (step) =>
    Promise.all(
      Array.from({ length: Math.min(at, items.length) }, async () => {
        while (next < items.length) {
          const x = items[next++];
          try {
            await call(x);
          } catch {
            failed.push(x);
          }
          step();
        }
      }),
    ),
  );
  return failed;
}
