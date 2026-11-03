// One row's layout, decided in one place (the owner, 2026-11-02: "there is an architectural deeper problem": the top
// row's parts each measured the others after the fact, and text could end up outside its glass). Every part of a row
// hands in its pieces, each with its own width; given the row's room, this decides what shows and how wide, at once,
// for all of them. Pure: lib/glassRow.svelte measures and applies.

/** One piece of a row: a breadcrumb's level, a title, the open field, a button a glass took. */
export type Piece = {
  id: string;
  /** Its own width laid out whole (its margins too). */
  width: number;
  /** Which goes first when there is no room: the highest. 0: never goes (cut first, where it may be, to its least). */
  rank: number;
  /** Narrower than whole down to this (its words cut with …) before it goes; none: whole or gone. */
  min?: number;
  /** Wider than whole up to this, from what is left (the open field). */
  max?: number;
};

export type Decision = {
  /** The pieces shown; the rest have gone. */
  shown: Set<string>;
  /** The width of each shown piece that is not its own (cut, or grown). */
  width: Map<string, number>;
};

/** How much spare room a piece that went needs to come back: never in and out at the same width. */
export const RETURN = 12;

/** The row's pieces within `room`: by rank, the highest first, each cut to its least where it can be before it goes;
 *  what is left grows what can grow. `before`: what showed before (a piece that went comes back only with RETURN to
 *  spare). */
export function layoutRow(room: number, pieces: readonly Piece[], before: ReadonlySet<string> | null = null): Decision {
  const width = new Map<string, number>();
  const shown = new Set(pieces.map((p) => p.id));
  const used = () => pieces.reduce((n, p) => n + (shown.has(p.id) ? (width.get(p.id) ?? p.width) : 0), 0);
  // What went before needs room to spare to come back.
  const slack = (p: Piece) => (before && !before.has(p.id) ? RETURN : 0);
  const fits = () => pieces.every((p) => !shown.has(p.id) || used() + slack(p) <= room + 0.5) && used() <= room + 0.5;
  // What never goes but may be cut (the open field) gives its room first, to its least, before anything else is cut or
  // goes (the owner, 2026-11-02: the open search ate the title; "title whole, field shrinks").
  for (const p of pieces) {
    const over = used() - room;
    if (over <= 0) break;
    if (p.rank !== 0 || p.min === undefined || p.min >= p.width) continue;
    width.set(p.id, Math.max(p.min, p.width - over));
  }
  const ranks = [...new Set(pieces.map((p) => p.rank).filter((r) => r > 0))].sort((a, b) => b - a);
  for (const rank of ranks) {
    if (fits()) break;
    const here = pieces.filter((p) => p.rank === rank);
    // Cut first, as far as it can be.
    for (const p of here) {
      if (p.min === undefined || p.min >= p.width) continue;
      const over = used() - room;
      if (over <= 0) break;
      width.set(p.id, Math.max(p.min, p.width - over));
    }
    if (fits()) break;
    for (const p of here) {
      shown.delete(p.id);
      width.delete(p.id);
      if (fits()) break;
    }
  }
  // Still over (only what never goes is left): cut what can be cut, whatever its rank.
  for (const p of pieces) {
    const over = used() - room;
    if (over <= 0) break;
    if (!shown.has(p.id) || p.min === undefined) continue;
    width.set(p.id, Math.max(p.min, (width.get(p.id) ?? p.width) - over));
  }
  // What is left grows what can grow, in order.
  for (const p of pieces) {
    if (!shown.has(p.id) || p.max === undefined) continue;
    const left = room - used();
    if (left <= 0) break;
    width.set(p.id, Math.min(p.max, (width.get(p.id) ?? p.width) + left));
  }
  return { shown, width };
}
