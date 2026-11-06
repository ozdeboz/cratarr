// The rule of the preferences on the account (decision 0101 point 14, lib/accountPrefs): per preference the latest
// change wins, the whole value; the first device to sign in to 0.16 while the server holds none sends its own once,
// and every other device takes the server's. Pure, so the tests take it as it is.

/** Whose this device's copy of the preferences is, and when each in it changed (epoch ms): 0 for a copy from before
 *  0.16 (its own account's, never changed since), none for a default (nothing of the account's to send). `account`:
 *  the account's key (lib/account); null: a copy from before 0.16, nobody's yet. */
export type Copy<K extends string> = { account: string | null; changed: Partial<Record<K, number>> };

/** The copy as `account` signs in on this device: a copy from before 0.16 becomes its own (each preference in it to be
 *  sent where the server holds none); another account's is none of its, so its preferences are let go (`fresh`: to
 *  their defaults, nothing shown and nothing sent). The same account's: as it is. */
export function signedIn<K extends string>(copy: Copy<K>, account: string, keys: readonly K[]): { copy: Copy<K>; fresh: boolean } {
  if (copy.account === account) return { copy, fresh: false };
  if (copy.account !== null) return { copy: { account, changed: {} }, fresh: true };
  const changed = { ...copy.changed };
  for (const k of keys) changed[k] ??= 0;
  return { copy: { account, changed }, fresh: false };
}

/** One preference as this device's copy (changed at `mine`) meets the account's (`held`; none: the server holds none):
 *  'take', the account's becomes this device's (it is later, or as late); 'send', this device's goes to the account
 *  (later, or the server holds none); null, neither has anything (a default here, none there). */
export function meet(held: { changed: number } | undefined, mine: number | undefined): 'take' | 'send' | null {
  if (held && held.changed >= (mine ?? 0)) return 'take';
  return mine === undefined ? null : 'send';
}

/** A value as JSON with its objects' keys in order, so that one value reads the same however it was put together (the
 *  server's copy and this device's). */
export const canon = (value: unknown): string =>
  JSON.stringify(value, (_, v: unknown) =>
    v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) : v,
  ) ?? 'null';
