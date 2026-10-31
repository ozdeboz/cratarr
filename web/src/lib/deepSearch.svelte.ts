// Deep (decision 0072): what the server's search finds of a kind by what it holds (a playlist or a crate by its tracks),
// with why; asked a moment after the pill's words stop changing. A page lets these through beside its own name filter.
import { search, type SearchKind } from './api';
import { pageView } from './pageView.svelte';
import { matchLine, type Matched } from './search';

/** One's why line (lib/search matchLine), or nothing; `answeredFor()`: the words the server last answered (or failed)
 *  for, for lib/search listShows. */
export type Deep = ((id: string) => string | undefined) & { answeredFor: () => string };
/** Called in a page's script: the subjects of `kind` the server finds not by their name, keyed by what follows `prefix`
 *  (`playlist:` → the playlist's id). */
export function deepMatches(kind: SearchKind, prefix: string): Deep {
  const found = $state({ by: new Map<string, Matched>(), q: '' });
  $effect(() => {
    const q = pageView.query.trim();
    if (!q) {
      found.by = new Map();
      found.q = '';
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
        },
        () => !gone && (found.q = q),
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
  return deep;
}
