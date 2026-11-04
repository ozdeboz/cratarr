// Where a track asked from a place at or past its end plays from (the 0.15 desktop app, 2026-11-04: resumed at 4:45 of
// a 4:24 track, it stalled silent with its clock counting on, and after a restart at 4:24 it never moved on). Such a
// place means the track was over while the player was away (a resume Cratarr reckoned on; it once asked for 36 minutes
// into a track of 2:23, 2026-10-24): the next track plays from its start, repeat one plays this one again, repeat all
// goes round to the first, and the queue's last stands at its own start, the queue having ended. Pure.

/** How close to its end (s) a place counts as past it: the last moment of a track is no place to start from. */
export const PAST_END_S = 1;

export type Repeat = 'off' | 'one' | 'all';
export type Resume = { index: number; from: number; play: boolean };

/** Where track `index` of a queue of `count` tracks, asked from `from` s and `length` s long where known, plays from,
 *  and whether it plays (`play`: as asked). A length not known yet keeps the place: the stream then says (an empty one,
 *  passed over). */
export function resumePlace(index: number, from: number, length: number | null | undefined, count: number, repeat: Repeat, play: boolean): Resume {
  const at = Number.isFinite(from) && from > 0 ? from : 0;
  // Its start is always a place to play from, however short the track.
  if (!length || at === 0 || at < length - PAST_END_S) return { index, from: at, play };
  if (repeat === 'one') return { index, from: 0, play };
  if (index + 1 < count) return { index: index + 1, from: 0, play };
  if (repeat === 'all') return { index: 0, from: 0, play };
  return { index, from: 0, play: false };
}
