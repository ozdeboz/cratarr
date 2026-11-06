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

/** Cratarr's sign-in page just sent this page here: who came is /api/me's to say. The name kept may be of someone whose
 *  session ended with no page open, so until it answers the page is nobody's, never theirs for a moment. A reload of
 *  such a page, later and perhaps out of reach, is no sign-in. */
function fromSignIn(): boolean {
  try {
    const open = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
    return open?.type !== 'reload' && new URL(document.referrer).pathname === '/signin';
  } catch {
    return false;
  }
}

const kept = typeof window === 'undefined' ? undefined : fromSignIn() ? null : stored();
let key: string | null = kept ?? null;
/** Learnt from /api/me in this page: the kept one read in late does not take its place. */
let learnt = false;

/** The account signed in: its name, '' with sign-in off; null for nobody (or nothing learnt on this device yet). */
export const accountKey = () => key;

/** An account's key as the device's stores name it, as the app does: `@name`, `@` with sign-in off, `-` for nobody. */
export const whoOf = (k: string | null) => (k === null ? '-' : `@${k}`);

/** A store's name for the account signed in now (`cratarr-shelf@name`). */
export const perAccount = (name: string) => name + whoOf(key);

/** Called once the account is known, and when another is (or nobody is). */
export const whenAccount = new Set<() => void>();

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
    if (now !== undefined && now !== key) location.reload();
  };
  addEventListener('storage', (e) => {
    if ((e as StorageEvent).key === LS) another();
  });
  addEventListener('pageshow', (e) => {
    if (!(e as PageTransitionEvent).persisted) return;
    // Its own sign-out cleared the name it would be compared by: its stores are nobody's, and its memory the account's.
    if (key === null) location.reload();
    else another();
  });
}

/** /api/me answered: `name` signed in (null: sign-in off, one user). */
export function accountLearnt(name: string | null) {
  learnt = true;
  set(name ?? '');
  toWorker(key);
}

/** Signing out here: nobody's from now on, so what is made before the next /api/me answers is no one's, never the one
 *  signed out's (632's review, 2026-11-02). */
export function accountForgotten() {
  learnt = false;
  set(null);
  toWorker(null);
}
