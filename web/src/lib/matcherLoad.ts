// The matcher's module (lib/matcher, matcher.wasm) fetched at the page's first idle moment after it loads (0.12), or at
// once when a search comes first: on the device, and in the service worker's keeping (its precache list holds every
// file of the build), before the network may go. Until it is in, a search leaves nothing out (lib/search), and the
// lists read again when it arrives (`ready`).
import url from './wasm/matcher.wasm?url';
import { instantiateMatcher, TextMatcher, type Matcher } from './matcher';
import { onMatcherWanted, useMatcher } from './search';

let state: 'idle' | 'loading' | 'in' = 'idle';
let done: (() => void) | undefined;
let module: Matcher | null = null;
const waiting: { resolve: (m: Matcher) => void; reject: (e: unknown) => void }[] = [];

function start() {
  if (state !== 'idle') return;
  state = 'loading';
  fetch(url)
    .then((r) => {
      if (!r.ok) throw new Error(`matcher.wasm: ${r.status}`);
      return instantiateMatcher(r);
    })
    .then(
      (m) => {
        state = 'in';
        module = m;
        for (const w of waiting.splice(0)) w.resolve(m);
        onMatcherWanted(null);
        useMatcher(new TextMatcher(m));
        done?.();
      },
      // Out of reach (offline before it was ever kept): asked again by the next search; who waits is told.
      (e) => {
        state = 'idle';
        for (const w of waiting.splice(0)) w.reject(e);
      },
    );
}

/** The matcher's module itself, for what else it holds (the record spread, lib/spread): at once once in, else loaded
 *  now and awaited. */
export function matcherModule(): Promise<Matcher> {
  if (module) return Promise.resolve(module);
  const p = new Promise<Matcher>((resolve, reject) => waiting.push({ resolve, reject }));
  start();
  return p;
}

/** Loads the matcher when the page is idle (a few seconds on at most), or at the first search before then; `ready`
 *  once it answers. */
export function loadMatcherWhenIdle(ready?: () => void) {
  if (typeof window === 'undefined' || typeof WebAssembly === 'undefined') return;
  done = ready;
  onMatcherWanted(start);
  if ('requestIdleCallback' in window) requestIdleCallback(start, { timeout: 3000 });
  else setTimeout(start, 1500);
}
