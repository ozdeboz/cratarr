// The rule of the preferences on the account (decision 0101 point 14, lib/accountPrefs): per preference the latest
// change wins, the whole value; a device sends what changed on it, and takes the server's otherwise. Pure, so the tests
// take it as it is.

/** Whose copy of the preferences this is, each account having its own on the device, and when each in it changed here
 *  (epoch ms; none for a default, nothing of the account's to send). `account`: the account's key (lib/account); null:
 *  nobody's, with no account to send to. */
export type Copy<K extends string> = { account: string | null; changed: Partial<Record<K, number>> };

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
