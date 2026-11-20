// Own music per account, on or off (0.17, the owner, 2026-11-08): Cratarr's own source keeps the accounts it is switched off for
// (`own_music_off`, the server's settings.rs). Off, the account owns none of it: no Your music anywhere and no Add to Your music.

/** The accounts own music is switched off for, as the source's values hold them. */
export const ownMusicOff = (values: Record<string, unknown>): string[] =>
  Array.isArray(values.own_music_off) ? values.own_music_off.filter((a): a is string => typeof a === 'string') : [];

/** The list with `account` switched on or off, each once, in the order switched off. */
export const withOwnMusic = (off: readonly string[], account: string, on: boolean): string[] =>
  on ? off.filter((a) => a !== account) : off.includes(account) ? [...off] : [...off, account];
