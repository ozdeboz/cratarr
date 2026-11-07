// Whose the things this device keeps are (the owner, 2026-11-02, a bug fixed in 0.13: a change or a play made under
// one account, still waiting when another signed in on the same device, was sent as the other's; decision 0101: each
// account's things on the device are its own, hidden while another is signed in or nobody is). The key is the
// account's name, or '' with sign-in off (one user); null for nobody (signed out, or nothing learnt on this device yet).
// Each kept thing to send carries the key it was made under and is sent only while that key is signed in (lib/later,
// lib/output; the rules are lib/outbox's); the stores kept per account name theirs with `perAccount` (lib/shelfStore
// and the rest), as the app does (android Who.kt). Learnt from /api/me (lib/me); out of reach at start, the last one
// learnt, kept here: in localStorage, so it is there before the first line of a store's own runs, and in the kept store
// where a browser refuses localStorage.
import { keep, readKept } from './keptStore';

const NAME = 'account';
const LS = 'cratarr-who';
const OWED = 'cratarr-sign-out-owed';

/** The key kept in localStorage: its name, `''`, or null for nobody; undefined where none is kept there (or it cannot
 *  be read), so the kept store's copy counts instead. Nobody kept there is an answer (signed out), never a gap. */
function stored(): string | null | undefined {
  try {
    const raw = localStorage.getItem(LS);
    if (raw === null) return undefined;
    const v = JSON.parse(raw);
    return typeof v === 'string' ? v : null;
  } catch {
    return undefined;
  }
}

// Cratarr's sign-in page writes nobody here (server/src/serve/signin.rs): a session over is known at the first line, whoever
// signs in next, and however long it is before Cratarr can be asked.
const kept = typeof window === 'undefined' ? undefined : stored();
let key: string | null = kept ?? null;
/** Learnt from /api/me in this page: the kept one read in late does not take its place. */
let learnt = false;

/** A sign-out begun that Cratarr has not said it took. Only Cratarr can end a session, its cookie out of the page's reach, so
 *  this is kept: until then the session stands, and what Cratarr answers is no one's. */
function owedKept() {
  try {
    return localStorage.getItem(OWED) !== null;
  } catch {
    return false;
  }
}
let owed = typeof window !== 'undefined' && owedKept();
export const signOutOwed = () => owed;

/** This page is going by the account's doing, a sign-out or another's in another tab: no leave for the browser to ask about
 *  (lib/output). On its Cancel the page would stay, its memory the one gone's over stores that are not. */
let going = false;
export const pageGoing = () => going;
export const pageGoes = () => void (going = true);

/** The account signed in: its name, '' with sign-in off; null for nobody (or nothing learnt on this device yet). */
export const accountKey = () => key;

/** An account's key as the device's stores name it, as the app does: `@name`, `@` with sign-in off, `-` for nobody. */
export const whoOf = (k: string | null) => (k === null ? '-' : `@${k}`);

/** A store's name for the account signed in now (`cratarr-shelf@name`). */
export const perAccount = (name: string) => name + whoOf(key);

/** Called once the account is known, and when another is (or nobody is). */
export const whenAccount = new Set<() => void>();

/** Called each time /api/me has answered here. A page opens as the name kept, so an ordinary open changes nothing and
 *  [whenAccount] does not hear of it: what must run at every open, once Cratarr has said who it is, is here. */
export const whenLearnt = new Set<() => void>();

/** What waits of the account, each to be sent, as far as it can be, before a sign-out ends it (decision 0101, point 13: "they
 *  go first if the device is online"; lib/signOut). */
export const whenLeaving = new Set<() => Promise<unknown>>();

function set(k: string | null) {
  if (k === key) return;
  key = k;
  try {
    localStorage.setItem(LS, JSON.stringify(k));
  } catch {
    /* the kept store has it */
  }
  void keep(NAME, k);
  for (const f of whenAccount) f();
}

// The kept store's copy only where localStorage keeps none (refused, or cleared): one that says nobody is not undone by
// an older copy there (the tablet, 2026-11-04).
if (typeof window !== 'undefined' && kept === undefined)
  readKept<unknown>(NAME).then((v) => {
    if (learnt || key !== null || typeof v !== 'string') return;
    set(v);
    toWorker(key);
  });

/** The service worker keeps and shows the last copies of this account's reads only (public/sw.js). */
function toWorker(k: string | null) {
  if (typeof navigator !== 'undefined') navigator.serviceWorker?.controller?.postMessage({ type: 'account', name: k });
}

if (typeof window !== 'undefined') {
  // Told before this page's first read, which it would otherwise answer as the last page's account.
  toWorker(key);
  // Another tab signed in or out, or this page came back from the browser's history after one did. What the page holds
  // in memory is the one before's and no store says so, so it starts over as the one there is now.
  const another = () => {
    const now = stored();
    if (now === undefined || now === key) return;
    pageGoes();
    location.reload();
  };
  addEventListener('storage', (e) => {
    if ((e as StorageEvent).key === LS) another();
  });
  addEventListener('pageshow', (e) => {
    if (!(e as PageTransitionEvent).persisted) return;
    // Its own sign-out cleared the name it would be compared by: its stores are nobody's, and its memory the account's.
    if (key !== null) return another();
    pageGoes();
    location.reload();
  });
}

/** /api/me answered: `name` signed in (null: sign-in off, one user). */
export function accountLearnt(name: string | null) {
  // The session it names is one signing out: an answer to a question asked before, or a session that stands for now.
  if (owed) return;
  learnt = true;
  set(name ?? '');
  toWorker(key);
  for (const f of whenLearnt) f();
}

/** Signing out here: nobody's from now on, so what is made before the next /api/me answers is no one's, never the one
 *  signed out's (632's review, 2026-11-02). */
export function accountForgotten() {
  learnt = false;
  set(null);
  toWorker(null);
}

/** A sign-out begins here: nobody's from now on, and owed until [signOutTold]. Kept first, so that a tab closed on the way
 *  owes it too. */
export function signOutBegun() {
  pageGoes();
  owed = true;
  try {
    localStorage.setItem(OWED, '1');
  } catch {
    /* owed for this page */
  }
  accountForgotten();
}

/** Cratarr has said the session is over. */
export function signOutTold() {
  pageGoes();
  owed = false;
  try {
    localStorage.removeItem(OWED);
  } catch {
    /* nothing kept */
  }
}
