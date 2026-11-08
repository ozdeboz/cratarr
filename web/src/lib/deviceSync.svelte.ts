// The library on this device kept up to date (D14). Whole once, then what changes; the rule is the rules crate's (`index::sync`), so
// the app's worker asks the same one, and this only fetches, keeps and says. In a browser: in the app the app keeps it (android's
// DeviceIndex), and this says what the app tells and passes on what is asked. Each account's apart (lib/indexStore), and nothing
// fetched for one lands under another.
//
// What is kept is bytes. An instance of index.wasm lives for one sync and is let go whole: nothing of it stays in memory, and the sounds
// are not loaded at all unless their feed has something to apply.
//
// A device holds three revs on one log, and no feed is given another's. Rows' and sounds' feeds are each a whole build on the server,
// so they are asked when `/api/changes` says something moved, by lib/changes. Covers' builds nothing, and asking it is what sets the
// maker going, so it is asked at every sync.
import { tick } from 'svelte';
import { accountKey, whenAccount, whoOf } from './account';
import { inApp, onApp, toApp } from './app.svelte';
import { CoverFrames } from './coverFrames';
import { newEngine, readsNow, whenReadsMove } from './deviceIndex.svelte';
import { indexShows, type IndexShows, type PartFacts } from './deviceIndexRule';
import type { IndexEngine, Network, Outcome, Part, Step } from './indexEngine';
import { keepCoversOf, putCovers, readBytes, readPart, writePart, type Kept, type PartName } from './indexStore';
import { prefs } from './prefs.svelte';
import { reach, refusedFor } from './reach.svelte';

/** When a start first looks at what to fetch: the rule's own first seconds (`FIRST_AFTER_MS`), so index.wasm is not loaded sooner to say so. */
export const FIRST_LOOK_MS = 10_000;
/** Hidden this long, a page coming back has started again: the rule's `ANEW_AFTER_MS`. */
export const ANEW_AFTER_MS = 5 * 60 * 60 * 1000;
/** Fetches for one part at one look: "start again", then its whole. */
const TURNS = 3;

/** What a part is, to fetching it. `always`: its feed builds nothing, and is asked at every sync; `frames`: it comes a frame at a time,
 *  and is kept by the frame; `stale`: it has no feed, and is fetched whole when it has stood a while (`Part::step_stale`). */
type Def = {
  name: PartName;
  whole: string;
  feed: (since: number) => string;
  save: (e: IndexEngine) => Uint8Array | null;
  rev: (e: IndexEngine) => number;
  version: (e: IndexEngine) => number;
  always?: boolean;
  frames?: boolean;
  stale?: boolean;
};

const PARTS: Def[] = [
  {
    name: 'index',
    whole: '/api/index',
    feed: (since) => `/api/index/changes?since=${since}`,
    save: (e) => e.saveIndex(),
    rev: (e) => e.indexRev(),
    version: (e) => e.indexVersion(),
  },
  // Apart, as they weigh more than the rest and change rarely.
  {
    name: 'sounds',
    whole: '/api/index/sounds',
    feed: (since) => `/api/index/sounds/changes?since=${since}`,
    save: (e) => e.saveSounds(),
    rev: (e) => e.soundsRev(),
    version: (e) => e.soundsVersion(),
  },
  {
    name: 'covers',
    whole: '/api/index/covers',
    feed: (since) => `/api/index/covers/changes?since=${since}`,
    save: () => null,
    rev: (e) => e.coversRev(),
    version: (e) => e.coversVersion(),
    always: true,
    frames: true,
  },
  // An account's fits, counts, crates and the rest a crate and a pick read (`index::own`).
  {
    name: 'own',
    whole: '/api/index/own',
    feed: () => '/api/index/own',
    save: () => null,
    rev: (e) => e.ownAt(),
    version: (e) => e.ownVersion(),
    stale: true,
  },
];

/** How it stands, for This device: when the index last stood where Cratarr does (ms), the rev it holds, the rev the covers hold, and
 *  what it is up to. */
export const indexSync = $state({ at: null as number | null, rev: null as number | null, covers: null as number | null, shows: 'idle' as IndexShows });

const nothingKept = (): Kept => ({ rev: null, at: null, wants: null, part: { whole: true, stopped: false, lost: 0, failed: 0, tried: null } });

/** When this start began, and how each part stands in it. */
let started = Date.now();
const parts = new Map<PartName, Part>();
let timer: ReturnType<typeof setTimeout> | null = null;
/** The look under way, and whether another is owed after it, told or not. */
let looking: Promise<void> | null = null;
let owed: boolean | null = null;

/** What a fetch costs now. With reads on the device nothing is fetched: nothing is there to answer. */
function networkNow(): Network {
  if (reach.noNetwork || readsNow().from === 'device') return 'off';
  const c = (navigator as Navigator & { connection?: { type?: string; saveData?: boolean } }).connection;
  return c?.type === 'cellular' || c?.saveData ? 'metered' : 'free';
}

type Fetched = { bytes: Uint8Array } | { outcome: Outcome };

async function fetched(e: IndexEngine, url: string, whole: boolean): Promise<Fetched> {
  try {
    const res = await fetch(url);
    const said = e.answer(res.status, whole);
    return said === 'bytes' ? { bytes: new Uint8Array(await res.arrayBuffer()) } : { outcome: said };
  } catch {
    return { outcome: 'lost' };
  }
}

/** What a fetch came to, and what of a part is kept for it. */
type Got = { outcome: Outcome; keep?: Partial<Kept>; bytes?: Uint8Array };

async function whole(e: IndexEngine, def: Def): Promise<Got> {
  const got = await fetched(e, def.whole, true);
  if (!('bytes' in got)) return got;
  const outcome = e.takeWhole(def.name, got.bytes);
  return outcome === 'took' ? { outcome, keep: { rev: def.rev(e), at: Date.now() }, bytes: got.bytes } : { outcome };
}

async function changes(e: IndexEngine, def: Def, who: string, kept: Kept): Promise<Got> {
  if (kept.rev === null) return { outcome: 'start-again' };
  const got = await fetched(e, def.feed(kept.rev), false);
  if (!('bytes' in got)) return got.outcome === 'took' ? { outcome: 'took', keep: { at: Date.now() } } : got;
  const held = await readBytes(who, def.name);
  if (!held) return { outcome: 'start-again' };
  const outcome = e.takeChanges(def.name, held, got.bytes);
  if (outcome !== 'took') return { outcome };
  const bytes = def.save(e);
  return bytes ? { outcome: 'took', keep: { rev: def.rev(e), at: Date.now() }, bytes } : { outcome: 'failed' };
}

/** Covers, into the store a frame at a time as they come: a bundle can be tens of MB, and is never held here whole. */
async function framed(e: IndexEngine, def: Def, who: string, whole: boolean, kept: Kept): Promise<Got> {
  if (!whole && kept.rev === null) return { outcome: 'start-again' };
  try {
    const res = await fetch(whole ? def.whole : def.feed(kept.rev!));
    const said = e.answer(res.status, whole);
    if (said !== 'bytes') return said === 'took' ? { outcome: 'took', keep: { at: Date.now() } } : { outcome: said };
    const frames = new CoverFrames();
    const body = res.body!.getReader();
    let taken = false;
    // A whole's records: what it no longer holds goes once it is all in, never before, so one cut short leaves what was held.
    const records = whole ? new Set<number>() : null;
    for (;;) {
      const { done, value } = await body.read();
      if (done) break;
      frames.push(value);
      const head = frames.head();
      if (head) {
        const outcome = e.takeCovers(head, whole);
        if (outcome !== 'took') {
          void body.cancel();
          return { outcome };
        }
        taken = true;
      }
      if (!taken) continue;
      const got = frames.take();
      for (const f of got) records?.add(f.record);
      if (!(await putCovers(who, got))) return { outcome: 'failed' };
    }
    if (!taken || frames.cut) return { outcome: e.takeCovers(new Uint8Array(0), whole) };
    if (records && !(await keepCoversOf(who, records))) return { outcome: 'failed' };
    return { outcome: 'took', keep: { rev: def.rev(e), at: Date.now() } };
  } catch {
    return { outcome: 'lost' };
  }
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
async function bringUp(e: IndexEngine, def: Def, who: string, told: boolean): Promise<Step> {
  for (let turn = 0; ; turn++) {
    if (whoOf(accountKey()) !== who) return { do: 'rest' };
    const kept = (await readPart(who, def.name)) ?? nothingKept();
    const part = partOf(e, def, kept);
    const step = def.stale ? e.stepStale(part, networkNow(), prefs.indexMobile, kept.at, Date.now()) : e.step(part, networkNow(), prefs.indexMobile, told || !!def.always, Date.now());
    if ((step.do !== 'whole' && step.do !== 'changes') || turn === TURNS) return step;
    indexSync.shows = 'fetching';
    if (step.do === 'whole' && kept.rev === null) await keepForGood();
    const got = def.frames ? await framed(e, def, who, step.do === 'whole', kept) : step.do === 'whole' ? await whole(e, def) : await changes(e, def, who, kept);
    // Another account came meanwhile: what was fetched is the one before's.
    if (whoOf(accountKey()) !== who) return { do: 'rest' };
    const after = e.after(part, got.outcome, Date.now());
    parts.set(def.name, after);
    const { started: _, ...stands } = after;
    const wrote = await writePart(who, def.name, { ...kept, ...got.keep, wants: def.version(e), part: stands }, got.bytes);
    if (def.name === 'index' && got.keep && wrote) Object.assign(indexSync, { at: got.keep.at ?? indexSync.at, rev: got.keep.rev ?? indexSync.rev });
    if (def.name === 'covers' && got.keep?.rev !== undefined && wrote) indexSync.covers = got.keep.rev;
    if (got.outcome === 'took' && wrote) return { do: 'rest' };
    // Storage refused it: nothing was taken, and a fetch at once would only be refused again.
    if (got.outcome === 'took') parts.set(def.name, e.after(part, 'failed', Date.now()));
  }
}

/** What the row says, from how the parts ended. */
function say(e: IndexEngine, steps: Step[]) {
  const facts = PARTS.flatMap((def, n): PartFacts[] => {
    const p = parts.get(def.name);
    // An account's own asks nothing and waits for nothing the row need say: the library is what it speaks of.
    return p && !def.stale ? [{ whole: p.whole, stopped: p.stopped, paused: e.paused(p), asks: steps[n].do === 'ask' }] : [];
  });
  indexSync.shows = indexShows(facts, prefs.indexMobile, networkNow() === 'metered');
}

function lookAt(when: number, told = false) {
  stopLooking();
  timer = setTimeout(() => void syncNow(told), Math.max(0, when - Date.now()));
}

async function look(told: boolean) {
  const who = whoOf(accountKey());
  try {
    const e = await newEngine();
    const steps: Step[] = [];
    for (const def of PARTS) steps.push(await bringUp(e, def, who, told));
    if (whoOf(accountKey()) === who) say(e, steps);
    const next = steps.flatMap((s) => (s.do === 'wait' ? [s.until] : []));
    // A wait comes to a feed that was told and has not answered yet: told it stays.
    if (next.length) lookAt(Math.min(...next), told);
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

/** The app told something. What the page just set goes first: the app reads the person's answer from the page's settings, and those
 *  go by an effect, after this turn. */
async function tellApp(type: string, told?: boolean) {
  await tick();
  toApp({ type, told });
}

/** Looks at what to fetch, and fetches it; in the app the app looks. `told`: the log said rows or sounds moved. Nothing for nobody, nor
 *  through a refusal. */
export function syncNow(told = false): Promise<void> {
  if (inApp) return tellApp('index-look', told);
  if (accountKey() === null || refusedFor() > 0) return Promise.resolve();
  if (looking) {
    owed = (owed ?? false) || told;
    return looking;
  }
  looking = look(told).finally(() => {
    looking = null;
    if (owed !== null) {
      const again = owed;
      owed = null;
      void syncNow(again);
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
  Object.assign(indexSync, { at: null, rev: null, covers: null, shows: 'idle' });
  if (accountKey() === null) return;
  const [kept, covers] = [await readPart(who, 'index'), await readPart(who, 'covers')];
  if (whoOf(accountKey()) !== who) return;
  indexSync.covers = covers?.rev ?? null;
  if (kept) Object.assign(indexSync, { at: kept.at, rev: kept.rev, shows: kept.part.stopped ? 'stopped' : 'idle' });
}

/** What `/api/changes` says of the library (lib/changes). Rows or sounds moved: their feeds are asked. The log gone past what the covers
 *  hold and no more: a cover moved, and says nothing of rows. */
export function indexStands(stands: { rev: number; changed: boolean; reset?: boolean } | undefined) {
  if (!stands || indexSync.rev === null) return Promise.resolve();
  if (stands.changed || stands.reset) return syncNow(true);
  return indexSync.covers === null || stands.rev > indexSync.covers ? syncNow() : Promise.resolve();
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
  if (inApp) return tellApp('index-again');
  for (const [name, p] of parts) if (p.stopped) parts.set(name, { ...p, stopped: false });
  return syncNow();
}

// What the app says of what it keeps (DeviceIndex.report): facts, and the row's word is said here, as in a browser.
onApp('device-index', (m) => {
  const told = m as unknown as { at: number | null; rev: number | null; covers: number | null; metered: boolean; parts: PartFacts[] };
  Object.assign(indexSync, { at: told.at, rev: told.rev, covers: told.covers, shows: indexShows(told.parts, prefs.indexMobile, told.metered) });
});

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
