// The search's matcher (lib/matcher: matcher.wasm, as the page has it once loaded), in for the tests that search. The
// tests share one process (npm test's --experimental-test-isolation=none), so it is put in once, here.
import { readFileSync } from 'node:fs';
import { instantiateMatcher, TextMatcher } from '../src/lib/matcher.ts';
import { useMatcher } from '../src/lib/search.ts';

export const matcher = new TextMatcher(await instantiateMatcher(readFileSync(new URL('../src/lib/wasm/matcher.wasm', import.meta.url))));
useMatcher(matcher);
