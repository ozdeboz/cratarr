// An account's room (AL01, decision 0084: a budget in GB per user, 100 unless set) and the disk floor (AL03: below it, free
// where users' music lands, users' new Wants wait), as the page says them. The server counts and says whether one is full;
// here only its numbers in words, never its rule worked out again. 1 GB = 10^9 bytes. Pure.

/** 1 GB, in bytes. */
export const GB = 1e9;

/** An account's room as the server says it (`/api/me`, `/api/users`): its size and what counts against it, in bytes, and
 *  whether it is full. None for an administrator, or where it could not be read. */
export type Room = { size: number; used: number; full: boolean };

/** The disk floor (`/api/settings/floor`): the floor in GB, the lowest free space where users' music lands in bytes (none where
 *  it cannot be read), and whether the server is short of space. */
export type Floor = { gb: number; free: number | null; short: boolean };

/** The decimals a room in GB is set with (OQ-R5: decimal GB; the panel's whole), at most to the byte. */
function placesOf(gb: number): number {
  let d = 0;
  while (d < 9 && Math.abs(gb * 10 ** d - Math.round(gb * 10 ** d)) > 1e-6) d++;
  return d;
}

/** "40 of 100 GB": what counts against a room of `gb`, `used` in bytes; "130 of 100 GB" over it (Q3). The used part is cut
 *  down to the room's own decimals, never rounded up, so it reads at or over the room exactly when the server says full
 *  (used ≥ size). No room (an administrator, or Cratarr out of reach): no line. */
export function roomLine(used: number | null | undefined, gb: number | null | undefined): string | null {
  if (used == null || gb == null) return null;
  const d = placesOf(gb);
  const shown = Math.floor(used / 10 ** (9 - d)) / 10 ** d;
  return `${shown} of ${Math.round(gb * 10 ** d) / 10 ** d} GB`;
}

/** A room's line: its numbers in words, none without a room or with Cratarr out of reach (AL04.5: as last seen it may be
 *  over by now). */
export const lineOfRoom = (room: Room | null | undefined, away = false): string | null => (away || !room ? null : roomLine(room.used, room.size / GB));

/** A room's size in GB, as its value sheet starts. */
export const gbOfRoom = (room: Room) => String(room.size / GB);

/** Free space in words: TB from a thousand GB, else whole GB, cut down, never rounded up past the floor. */
const freeText = (b: number) => (b >= 1e12 ? `${Math.floor(b / 1e11) / 10} TB` : `${Math.floor(b / GB)} GB`);

/** The floor's row in Settings: its value, the free space in its line, warned while short; none with Cratarr out of reach. */
export function floorRow(f: Floor | null | undefined, away = false): { value: string; line: string | null; tone: 'warn' | null } | null {
  if (away || !f) return null;
  return { value: `${f.gb} GB`, line: f.free === null ? null : `${freeText(f.free)} free`, tone: f.short ? 'warn' : null };
}
