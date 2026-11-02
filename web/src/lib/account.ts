// Whose the things this device keeps to send later are (the owner, 2026-11-02, a bug fixed in 0.13: a change or a play
// made under one account, still waiting when another signed in on the same device, was sent as the other's). The key is
// the account's name, or '' with sign-in off (one user). Each kept thing carries the key it was made under and is sent
// only while that key is signed in (lib/later, lib/output; the rules are lib/outbox's). Learnt from /api/me (lib/me);
// out of reach at start, the last one learnt, kept here.
import { keep, readKept } from './keptStore';

const NAME = 'account';

let key: string | null = null;
/** Learnt from /api/me in this page: the kept one read in late does not take its place. */
let learnt = false;

/** The account signed in: its name, '' with sign-in off; null while not known (nothing learnt on this device yet). */
export const accountKey = () => key;

/** Called once the account is known, and when another is. */
export const whenAccount = new Set<() => void>();

function set(k: string) {
  if (k === key) return;
  key = k;
  for (const f of whenAccount) f();
}

if (typeof window !== 'undefined')
  readKept<unknown>(NAME).then((v) => {
    if (!learnt && typeof v === 'string') set(v);
  });

/** /api/me answered: `name` signed in (null: sign-in off, one user). */
export function accountLearnt(name: string | null) {
  learnt = true;
  set(name ?? '');
  void keep(NAME, key);
}

/** Signing out here: the account is forgotten, kept one too, so what is made before the next /api/me answers waits
 *  unstamped and takes that account's key, not the one signed out (632's review, 2026-11-02). */
export function accountForgotten() {
  learnt = false;
  key = null;
  void keep(NAME, null);
}
