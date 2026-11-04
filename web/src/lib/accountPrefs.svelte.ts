// The person's preferences on their account (decision 0101 point 14): the pinned playlists, the libraries switched
// off, the lists' filters and Radio's last choice follow the account from device to device, through the server's
// /api/me/preferences; offline they are this device's copy (the prefs) as last known. The rule is
// lib/accountPrefsRule's: the latest change wins, the whole value. Another account's copy is never shown or sent:
// someone else signing in here starts from the defaults until theirs come. A change on another of the account's
// devices comes on the live channel (`changed`: preferences), and the copy is compared again whenever the channel is
// back.
import { untrack } from 'svelte';
import { accountKey, whenAccount } from './account';
import { canon, meet, signedIn, type Copy } from './accountPrefsRule';
import { getPreferences, putPreference, type HeldPreference } from './api';
import { on, stream } from './events.svelte';
import { fromSaved, prefs, type Prefs } from './prefs.svelte';

/** Each preference on the account, by its key there: the prefs' field that holds it on this device. */
const HELD = { hidden_libraries: 'hiddenLibraries', pinned: 'pinned', filters: 'filters', radio_last: 'radioLast' } as const satisfies Record<string, keyof Prefs>;
type Key = keyof typeof HELD;
const KEYS = Object.keys(HELD) as Key[];

const COPY = 'cratarr-prefs-account';

function readCopy(): Copy<Key> {
  try {
    const c = JSON.parse(localStorage.getItem(COPY) ?? 'null');
    if (c && typeof c === 'object' && (typeof c.account === 'string' || c.account === null)) {
      const changed: Copy<Key>['changed'] = {};
      for (const k of KEYS) if (Number.isFinite(c.changed?.[k])) changed[k] = c.changed[k];
      return { account: c.account, changed };
    }
  } catch {
    /* none kept */
  }
  return { account: null, changed: {} };
}

let copy = readCopy();
function keepCopy() {
  try {
    localStorage.setItem(COPY, JSON.stringify(copy));
  } catch {
    /* not kept */
  }
}

const set = <F extends keyof Prefs>(field: F, value: Prefs[F]) => (prefs[field] = value);
const now = (k: Key) => canon(prefs[HELD[k]]);

/** Each preference as last seen here (read, taken from the account, or sent), so that only this device's changes are
 *  sent. */
const seen = {} as Record<Key, string>;
for (const k of KEYS) seen[k] = now(k);

/** The account's value made this device's. */
function take(k: Key, held: HeldPreference) {
  const value = fromSaved(HELD[k], held.value);
  seen[k] = canon(value);
  set(HELD[k], value);
  copy.changed[k] = held.changed;
}

const timers: Partial<Record<Key, ReturnType<typeof setTimeout>>> = {};

/** This device's value sent to the account, after `wait` ms (changes close together are sent once); not reached, it
 *  goes when the copy is compared again (the channel back, the next start). */
function send(k: Key, wait = 0) {
  clearTimeout(timers[k]);
  timers[k] = setTimeout(async () => {
    delete timers[k];
    const account = accountKey();
    if (account === null || account !== copy.account) return;
    const value = $state.snapshot(prefs[HELD[k]]);
    const changed = copy.changed[k] || Date.now();
    try {
      const held = await putPreference(k, value, changed);
      if (accountKey() !== account) return;
      // What the server holds afterwards: this value, or a later change of another device's, unless this one has
      // changed again meanwhile (that goes next).
      if (canon(held.value) === canon(value)) copy.changed[k] = held.changed;
      else if (now(k) === canon(value)) take(k, held);
      keepCopy();
    } catch {
      /* not reached */
    }
  }, wait);
}

/** This device's copy compared with the account's: whichever changed later wins, each preference by itself. */
async function compare() {
  const account = accountKey();
  if (account === null) return;
  const was = signedIn(copy, account, KEYS);
  if (was.fresh)
    for (const k of KEYS) {
      clearTimeout(timers[k]);
      const value = fromSaved(HELD[k], undefined);
      seen[k] = canon(value);
      set(HELD[k], value);
    }
  if (was.copy !== copy) {
    copy = was.copy;
    keepCopy();
  }
  let values: Record<string, HeldPreference>;
  try {
    values = await getPreferences();
  } catch {
    return; // not reached: this device's copy stands until it is compared again
  }
  if (accountKey() !== account) return;
  for (const k of KEYS) {
    const held = values[k];
    const step = meet(held, copy.changed[k]);
    if (step === 'take') {
      if (canon(held.value) === now(k)) copy.changed[k] = held.changed;
      else take(k, held);
    } else if (step === 'send') send(k);
  }
  keepCopy();
}

// One comparison at a time; asked again while one runs, it runs once more after it.
let running: Promise<void> | null = null;
let again = false;
function sync() {
  if (running) {
    again = true;
    return;
  }
  running = (async () => {
    do {
      again = false;
      await compare();
    } while (again);
  })().finally(() => (running = null));
}

whenAccount.add(sync);
on<{ area?: string }>('changed', ({ area }) => void (area === 'preferences' && sync()));
$effect.root(() => {
  // The channel back: what changed meanwhile is not on it, so the copy is compared again.
  $effect(() => {
    if (stream.connected) untrack(sync);
  });
  // A change of this device's: kept with its time, and sent.
  for (const k of KEYS)
    $effect(() => {
      const value = now(k);
      untrack(() => {
        if (value === seen[k]) return;
        seen[k] = value;
        copy.changed[k] = Date.now();
        keepCopy();
        send(k, 500);
      });
    });
});
