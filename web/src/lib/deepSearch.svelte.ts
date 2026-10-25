// Deep (decision 0072): what the server's search finds of a kind by what it holds (a playlist or a crate by its tracks),
// with why; asked a moment after the pill's words stop changing. A page lets these through beside its own name filter.
import { search, type SearchKind } from './api';
import { pageView } from './pageView.svelte';
import type { Matched } from './search';

/** Called in a page's script: the subjects of `kind` the server finds not by their name, keyed by what follows `prefix`
 *  (`playlist:` → the playlist's id); the returned function gives one's why line, or nothing. */
export function deepMatches(kind: SearchKind, prefix: string): (id: string) => string | undefined {
  const found = $state({ by: new Map<string, Matched>() });
  $effect(() => {
    const q = pageView.query.trim();
    if (!q) {
      found.by = new Map();
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
        },
        () => {},
      );
    }, 250);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  return (id) => {
    const d = pageView.query.trim() ? found.by.get(id) : undefined;
    return d ? `${d.text}${d.count && d.count > 1 ? ` · ${d.count} tracks` : ''}` : undefined;
  };
}
