// Deep (decision 0072): what the server's search finds of a kind by what it holds (a playlist or a crate by its tracks),
// with why; asked a moment after the pill's words stop changing. A page lets these through beside its own name filter.
import { search, type SearchKind } from './api';
import { pageView } from './pageView.svelte';
import { matchLine, type Matched } from './search';

/** One's why line (lib/search matchLine), or nothing; `answeredFor()`: the words the server last answered for, and
 *  `failedFor()` the words it last failed for, for lib/search listShows; `error()`: why it failed, `again()` asks again
 *  (the owner, 2026-11-02: a failure is not an answer, so never "No … match." after one). */
export type Deep = ((id: string) => string | undefined) & {
  answeredFor: () => string;
  failedFor: () => string;
  error: () => string | null;
  again: () => void;
};
/** Called in a page's script: the subjects of `kind` the server finds not by their name, keyed by what follows `prefix`
 *  (`playlist:` → the playlist's id). */
export function deepMatches(kind: SearchKind, prefix: string): Deep {
  const found = $state({ by: new Map<string, Matched>(), q: '', failed: '', error: null as string | null, asks: 0 });
  $effect(() => {
    const q = pageView.query.trim();
    void found.asks;
    if (!q) {
      found.by = new Map();
      found.q = '';
      found.failed = '';
      found.error = null;
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      search(q, [kind], 100).then(
        (r) => {
          if (gone) return;
          const m = new Map<string, Matched>();
          for (const it of r.groups[kind]?.items ?? []) {
            const on = it.matched.find((x) => x.on !== 'name');
            if (on && it.subject.startsWith(prefix)) m.set(it.subject.slice(prefix.length), on);
          }
          found.by = m;
          found.q = q;
          found.failed = '';
          found.error = null;
        },
        (e) => {
          if (gone) return;
          found.failed = q;
          found.error = e instanceof Error ? e.message : 'Unexpected error';
        },
      );
    }, 250);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  const deep = ((id: string) => {
    const d = pageView.query.trim() ? found.by.get(id) : undefined;
    return d ? (matchLine([d]) ?? undefined) : undefined;
  }) as Deep;
  deep.answeredFor = () => found.q;
  deep.failedFor = () => found.failed;
  deep.error = () => found.error;
  // Asked again: the wait till its answer, not the failure.
  deep.again = () => ((found.failed = ''), found.asks++);
  return deep;
}
