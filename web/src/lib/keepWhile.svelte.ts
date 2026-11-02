// The one loading rule's keeping (lib/loader): a value as it was last shown while what replaces it is waited for, and
// whether the loader is due. In a component's setup (its effect lives with it).
import { untrack } from 'svelte';
import { LOADER_AFTER_MS, waitShows } from './loader';

export type Kept<T> = {
  /** The value: the live one when nothing is waited for, else the last one shown before the wait. */
  readonly value: T;
  /** The wait has lasted LOADER_AFTER_MS: the loader takes the place of the kept value. */
  readonly loader: boolean;
};

/** `value` kept as it last was while `waiting()`; `key()` names what is waited for (the words as they are now): a new one
 *  starts the wait's count again, so typing on keeps what was shown, and the loader comes LOADER_AFTER_MS after the
 *  last change only if its answer is still not in. */
export function keepWhile<T>(value: () => T, waiting: () => boolean, key: () => unknown = () => null): Kept<T> {
  let kept = $state.raw<T>(untrack(value));
  let since = $state<number | null>(null);
  let now = $state(0);
  // Before the DOM is updated, so what is kept is there in the same frame the wait begins.
  $effect.pre(() => {
    if (!waiting()) {
      kept = value();
      since = null;
      return;
    }
    void key();
    const at = performance.now();
    untrack(() => {
      since = at;
      now = at;
    });
    const t = setTimeout(() => (now = Math.max(performance.now(), at + LOADER_AFTER_MS)), LOADER_AFTER_MS);
    return () => clearTimeout(t);
  });
  return {
    get value() {
      return waiting() ? kept : value();
    },
    get loader() {
      return waiting() && waitShows(since, now) === 'loader';
    },
  };
}
