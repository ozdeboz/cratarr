// The library on this device kept up to date (D14). Whole once, then what changes; the rule is the rules crate's (`index::sync`), so
// the app's worker asks the same one, and this only fetches, keeps and says. In a browser: in the app the app keeps it (D15), and the
// page reads what it holds. Each account's apart (lib/indexStore), and nothing fetched for one lands under another.
//
// What is kept is bytes. An instance of index.wasm lives for one sync and is let go whole: nothing of it stays in memory, and the sounds
// are not loaded at all unless their feed has something to apply.
import { accountKey, whenAccount, whoOf } from './account';
import { inApp } from './app.svelte';
import { newEngine, readsNow, whenReadsMove } from './deviceIndex.svelte';
import type { IndexShows } from './deviceIndexRule';
import type { IndexEngine, Network, Outcome, Part, Refused, Step } from './indexEngine';
import { readPart, writePart, type Kept, type PartName } from './indexStore';
import { prefs } from './prefs.svelte';
import { notReached, reach, refusedFor } from './reach.svelte';

/** When a start first looks at what to fetch: the rule's own first seconds (`FIRST_AFTER_MS`), so index.wasm is not loaded sooner to say so. */
export const FIRST_LOOK_MS = 10_000;
/** Hidden this long, a page coming back has started again: the rule's `ANEW_AFTER_MS`. */
export const ANEW_AFTER_MS = 5 * 60 * 60 * 1000;
/** Fetches for one part at one look: "start again", then its whole. */
const TURNS = 3;

/** What a part is, to fetching it. D12's fits and D13's small covers are more rows here, each with its own rev, as the sounds are. */
type Def = {
  name: PartName;
  whole: string;
  feed: (since: number) => string;
  load: (e: IndexEngine, bytes: Uint8Array) => Refused | null;
  apply: (e: IndexEngine, bytes: Uint8Array) => Refused | null;
  save: (e: IndexEngine) => Uint8Array | null;
  rev: (e: IndexEngine) => number;
  version: (e: IndexEngine) => number;
};

const PARTS: Def[] = [
  {
    name: 'index',
    whole: '/api/index',
    feed: (since) => `/api/index/changes?since=${since}`,
    load: (e, bytes) => e.loadIndex(bytes),
    apply: (e, bytes) => e.applyIndex(bytes),
    save: (e) => e.saveIndex(),
    rev: (e) => e.indexRev(),
    version: (e) => e.indexVersion(),
  },
  // Apart, as they weigh more than the rest and change rarely.
  {
    name: 'sounds',
    whole: '/api/index/sounds',
    feed: (since) => `/api/index/sounds/changes?since=${since}`,
    load: (e, bytes) => e.loadSounds(bytes),
    apply: (e, bytes) => e.applySounds(bytes),
    save: (e) => e.saveSounds(),
    rev: (e) => e.soundsRev(),
    version: (e) => e.soundsVersion(),
  },
];

/** How it stands, for This device: when the index last stood where Cratarr does (ms), the rev it holds, and what it is up to. */
export const indexSync = $state({ at: null as number | null, rev: null as number | null, shows: 'idle' as IndexShows });

const nothingKept = (): Kept => ({ bytes: null, rev: null, at: null, wants: null, part: { whole: true, stopped: false, lost: 0, failed: 0, tried: null } });

/** When this start began, and how each part stands in it. */
let started = Date.now();
const parts = new Map<PartName, Part>();
let timer: ReturnType<typeof setTimeout> | null = null;
/** The look under way, and whether another is owed after it. */
let looking: Promise<void> | null = null;
let owed = false;

/** What a fetch costs now. With reads on the device nothing is fetched: nothing is there to answer. */
function networkNow(): Network {
  if (reach.noNetwork || readsNow().from === 'device') return 'off';
  const c = (navigator as Navigator & { connection?: { type?: string; saveData?: boolean } }).connection;
  return c?.type === 'cellular' || c?.saveData ? 'metered' : 'free';
}

type Fetched = { bytes: Uint8Array } | { outcome: Outcome | 'same' };

async function fetched(url: string): Promise<Fetched> {
  try {
    const res = await fetch(url);
    if (res.status === 204) return { outcome: 'same' };
    if (res.status === 410) return { outcome: 'start-again' };
    // No answer of Cratarr's, or none it can give this session: the connection's, and the sign-in's own to say.
    if (notReached(res.status) || res.status === 401) return { outcome: 'lost' };
    if (!res.ok) return { outcome: 'failed' };
    return { bytes: new Uint8Array(await res.arrayBuffer()) };
  } catch {
    return { outcome: 'lost' };
  }
}

/** What a fetch came to, and what of a part is kept for it. */
type Got = { outcome: Outcome; keep?: Partial<Kept> };

async function whole(e: IndexEngine, def: Def): Promise<Got> {
  const got = await fetched(def.whole);
  if (!('bytes' in got)) return { outcome: got.outcome === 'same' || got.outcome === 'start-again' ? 'failed' : got.outcome };
  const refused = def.load(e, got.bytes);
  if (refused) return { outcome: refused.why === 'version' ? 'version' : 'failed' };
  return { outcome: 'took', keep: { bytes: got.bytes, rev: def.rev(e), at: Date.now() } };
}

async function changes(e: IndexEngine, def: Def, kept: Kept): Promise<Got> {
  if (!kept.bytes || kept.rev === null) return { outcome: 'start-again' };
  const got = await fetched(def.feed(kept.rev));
  if (!('bytes' in got)) return got.outcome === 'same' ? { outcome: 'took', keep: { at: Date.now() } } : { outcome: got.outcome };
  // What cannot go on, of whichever version, is fetched whole: only bytes fetched whole can say the two differ for good.
  if (def.load(e, kept.bytes) || def.apply(e, got.bytes)) return { outcome: 'start-again' };
  const bytes = def.save(e);
  return bytes ? { outcome: 'took', keep: { bytes, rev: def.rev(e), at: Date.now() } } : { outcome: 'failed' };
}

/** The storage asked to stay, at the first download: a browser lets go of what it was not told to keep. */
async function keepForGood() {
  if (prefs.storageAsked) return;
  prefs.storageAsked = true;
  await navigator.storage?.persist?.().catch(() => false);
}

/** A part as this start takes it from what is kept. */
function partOf(e: IndexEngine, def: Def, kept: Kept): Part {
  const known = parts.get(def.name);
  if (known) return known;
  const part = e.after({ ...kept.part, started }, kept.wants === def.version(e) ? 'start' : 'start-over-another', started);
  parts.set(def.name, part);
  return part;
}

/** One part as far as it goes now: what it was last told to do. */
async function bringUp(e: IndexEngine, def: Def, who: string): Promise<Step> {
  for (let turn = 0; ; turn++) {
    if (whoOf(accountKey()) !== who) return { do: 'rest' };
    const kept = (await readPart(who, def.name)) ?? nothingKept();
    const part = partOf(e, def, kept);
    const step = e.step(part, networkNow(), prefs.indexMobile, Date.now());
    if ((step.do !== 'whole' && step.do !== 'changes') || turn === TURNS) return step;
    indexSync.shows = 'fetching';
    if (step.do === 'whole' && !kept.bytes) await keepForGood();
    const got = step.do === 'whole' ? await whole(e, def) : await changes(e, def, kept);
    // Another account came meanwhile: what was fetched is the one before's.
    if (whoOf(accountKey()) !== who) return { do: 'rest' };
    const after = e.after(part, got.outcome, Date.now());
    parts.set(def.name, after);
    const { started: _, ...stands } = after;
    const wrote = await writePart(who, def.name, { ...kept, ...got.keep, wants: def.version(e), part: stands });
    if (def.name === 'index' && got.keep && wrote) Object.assign(indexSync, { at: got.keep.at ?? indexSync.at, rev: got.keep.rev ?? indexSync.rev });
    if (got.outcome === 'took' && wrote) return { do: 'rest' };
    // Storage refused it: nothing was taken, and a fetch at once would only be refused again.
    if (got.outcome === 'took') parts.set(def.name, e.after(part, 'failed', Date.now()));
  }
}

/** What the row says, from how the parts ended. */
function say(e: IndexEngine, steps: Step[]) {
  const all = [...parts.values()];
  const shows: IndexShows = all.some((p) => p.stopped)
    ? 'stopped'
    : steps.some((s) => s.do === 'ask')
      ? 'asks'
      : all.some((p) => p.whole) && prefs.indexMobile === 'no' && networkNow() === 'metered'
        ? 'wifi'
        : all.some((p) => e.paused(p))
          ? 'paused'
          : 'idle';
  indexSync.shows = shows;
}

function lookAt(when: number) {
  stopLooking();
  timer = setTimeout(() => void syncNow(), Math.max(0, when - Date.now()));
}

async function look() {
  const who = whoOf(accountKey());
  try {
    const e = await newEngine();
    const steps: Step[] = [];
    for (const def of PARTS) steps.push(await bringUp(e, def, who));
    if (whoOf(accountKey()) === who) say(e, steps);
    const next = steps.flatMap((s) => (s.do === 'wait' ? [s.until] : []));
    if (next.length) lookAt(Math.min(...next));
  } catch {
    /* index.wasm did not come: the next look */
  }
}

/** The look under way, done. */
export const lookDone = () => looking ?? Promise.resolve();

/** No look is due. */
export function stopLooking() {
  if (timer !== null) clearTimeout(timer);
  timer = null;
}

/** Looks at what to fetch, and fetches it. Nothing in the app, nor for nobody, nor through a refusal. */
export function syncNow(): Promise<void> {
  if (inApp || accountKey() === null || refusedFor() > 0) return Promise.resolve();
  if (looking) {
    owed = true;
    return looking;
  }
  looking = look().finally(() => {
    looking = null;
    if (owed) {
      owed = false;
      void syncNow();
    }
  });
  return looking;
}

/** A start: what went wrong is forgotten, and the first look comes after its first seconds. */
export function startAnew(now = Date.now()) {
  started = now;
  parts.clear();
  lookAt(now + FIRST_LOOK_MS);
}

/** What was held of this account's, shown. */
async function shownFromStore() {
  const who = whoOf(accountKey());
  Object.assign(indexSync, { at: null, rev: null, shows: 'idle' });
  if (accountKey() === null) return;
  const kept = await readPart(who, 'index');
  if (whoOf(accountKey()) === who && kept) Object.assign(indexSync, { at: kept.at, rev: kept.rev, shows: kept.part.stopped ? 'stopped' : 'idle' });
}

/** What `/api/changes` says of the library (lib/changes): moved, its rows are fetched. */
export function indexStands(stands: { changed: boolean; reset?: boolean } | undefined) {
  return stands && indexSync.rev !== null && (stands.changed || stands.reset) ? syncNow() : Promise.resolve();
}

/** The person's answer to fetching it whole on mobile data. */
export function answerMobile(yes: boolean) {
  prefs.indexMobile = yes ? 'yes' : 'no';
  if (yes) return syncNow();
  indexSync.shows = 'wifi';
  return Promise.resolve();
}

/** The row tapped: asked again, what stopped tried again. */
export function tryIndexAgain() {
  if (prefs.indexMobile === 'no') prefs.indexMobile = 'unasked';
  for (const [name, p] of parts) if (p.stopped) parts.set(name, { ...p, stopped: false });
  return syncNow();
}

if (typeof window !== 'undefined' && !inApp) {
  void shownFromStore();
  startAnew();
  whenAccount.add(() => {
    void shownFromStore();
    startAnew();
  });
  whenReadsMove.add(() => void syncNow());
  (navigator as Navigator & { connection?: EventTarget }).connection?.addEventListener('change', () => void syncNow());
  let hiddenAt: number | null = null;
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) hiddenAt = Date.now();
    else if (hiddenAt !== null && Date.now() - hiddenAt >= ANEW_AFTER_MS) startAnew();
  });
}
