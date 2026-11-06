// A record and its copies, the page's rules, pure (docs/records-and-copies.md, "On the page"): whether a record shows,
// the dots on its card, and which copy plays. `on` says whether a library is switched on (lib/sources: `shownIn`), the
// switches being the page's: one changed reaches the server later, and the list is not read again for it.
//
// A list says `copies`, or until the server folds its records (and for a server of another kind) the one `library`, or
// neither: a list with no libraries (the device's, a server's in its mode, Discover).
import type { AlbumCopy, LibraryInfo } from './api';

export type Held = { library?: string; copies?: AlbumCopy[] };
export type Dot = { id: string; colour: string; name: string };
type On = (library: string) => boolean;

/** The libraries holding a record, each once: two folders of one library are two copies, and one place to name. */
export const librariesOf = (a: Held): string[] => [...new Set(a.copies?.map((c) => c.library) ?? (a.library ? [a.library] : []))];

/** Whether a record shows: any of its copies in a library switched on. An Incoming copy counts: with the kept library
 *  off, the Library is where it is found. A list with no libraries has nothing to switch off; a record with no copies
 *  (`copies: []`, one not had) is in none. */
export function recordOn(a: Held, on: On): boolean {
  if (!a.copies && !a.library) return true;
  return librariesOf(a).some(on);
}

/** The dots on a record's card, one per library holding it, in the list's order: none while fewer than two libraries
 *  are on, as every record would carry the same. A library counts as on before its first record, own music too.
 *  Copies in libraries switched off say nothing while another is on, and a card reached with all of them off (Home's
 *  crates, a search) names where it lies. An Incoming copy gets none beside a kept one: it waits to be added. */
export function dotsOf(a: Held, info: LibraryInfo[] | undefined, on: On): Dot[] {
  if (!info || info.filter((l) => on(l.id)).length < 2) return [];
  const held = librariesOf(a);
  const lit = held.some(on) ? held.filter(on) : held;
  const shown = lit.flatMap((id) => info.find((l) => l.id === id) ?? []);
  const kept = shown.some((l) => l.kind !== 'incoming');
  return shown.flatMap(({ id, kind, colour, name }) => (colour && !(kept && kind === 'incoming') ? { id, colour, name } : []));
}

/** The copy that plays, as the switches stand now. The server decides (`copy_for`) and lists the copies in its order
 *  for this viewer, off or not, so the first one on is it. Only rule 4 is the page's: an Incoming copy plays when
 *  picked, or when the server says so (a record only in Incoming). Null: none can, or the list names no copies, and
 *  the record's own `format` and `bitrate_kbps` are all there is. */
export function copyPlaying(a: Held, info: LibraryInfo[] | undefined, on: On): AlbumCopy | null {
  const incoming = (c: AlbumCopy) => info?.find((l) => l.id === c.library)?.kind === 'incoming';
  return a.copies?.find((c) => on(c.library) && (c.picked || c.plays || !incoming(c))) ?? null;
}
