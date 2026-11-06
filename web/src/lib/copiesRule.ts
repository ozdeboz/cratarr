// A record and its copies, the page's rules, pure (docs/records-and-copies.md, "On the page"): whether a record shows,
// the dots on its card, which copy plays, and what each copy allows. `on` says whether a library is switched on (lib/sources: `shownIn`), the
// switches being the page's: one changed reaches the server later, and the list is not read again for it.
//
// A list says `copies`, or until the server folds its records (and for a server of another kind) the one `library`, or
// neither: a list with no libraries (the device's, a server's in its mode, Discover).
import type { AlbumCopy, AlbumsSource, LibraryInfo, ListedAlbum } from './api';

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

/** What one copy of a record allows (decision 0034): moves into the server's other used libraries, Replace for an
 *  upgrade, Delete; each only when Cratarr can write its folder, and with a job in the conversion queue only Cancel. */
export type AlbumOffer = {
  /** The copy's library, as an action names it (`from`): none for an album of a list with no libraries. */
  library?: string;
  from: LibraryInfo | null;
  /** How a menu names the copy. */
  name: string;
  /** The libraries it can move into. */
  targets: LibraryInfo[];
  /** The main libraries it can Replace a copy in (decision 0101 point 2). */
  replaceInto: LibraryInfo[];
  deletable: boolean;
  job: number | null;
};

/** Its move into `to` as the menus say it (the owner, 2026-11-04): an Incoming record, or one into someone's own music,
 *  is added to it ("Add to Your music", "Add to Music"); one between main libraries moves. */
export const adds = (from: LibraryInfo | null, to: LibraryInfo) => from?.kind === 'incoming' || to.kind === 'own';

type Offered = Pick<AlbumsSource, 'library_info' | 'verdicts' | 'deletable' | 'keepable' | 'keep_to'>;
type Listed = Pick<ListedAlbum, 'id' | 'library' | 'copies'>;
type Rights = Pick<AlbumCopy, 'deletable' | 'keepable' | 'keep_to'> & { library?: string };

/** Each library's copy of a record, once: of two folders of one library `from` names the first. Another server's album
 *  says what may be done with it in its list, by id, and lies in its one library. */
function copiesIn(s: Offered, a: Listed): Rights[] {
  if (!a.copies) return [{ library: a.library, deletable: s.deletable.includes(a.id), keepable: s.keepable.includes(a.id), keep_to: s.keep_to[a.id] ?? null }];
  return a.copies.filter((c, i) => a.copies!.findIndex((x) => x.library === c.library) === i);
}

/** One offer per copy, in the list's order. A library already holding a copy is no target: that is Replace, not Move.
 *  An Incoming copy's Add to goes first where it goes by default (`keep_to`: the requester's own music, else the first
 *  main library), the others after it in the list's order. An upgrade's Replace takes the place of its other copy, so
 *  of any copy in a main library. `jobOf`: the conversion at work on a library's copy. */
export function offersOf(s: Offered, a: Listed, admin: boolean, jobOf: (library?: string) => number | null): AlbumOffer[] {
  const info = (id?: string) => s.library_info.find((l) => l.id === id) ?? null;
  const verdict = s.verdicts[a.id];
  const into = verdict?.replaceable_into ?? [];
  const held = librariesOf(a);
  const mains = held.filter((id) => info(id)?.kind === 'main');
  const replacing = verdict?.kind === 'upgrade' ? [...new Set([...into, ...mains])].flatMap((id) => info(id) ?? []) : [];
  const others = s.library_info.filter((l) => !held.includes(l.id) && !into.includes(l.id));
  return copiesIn(s, a).map((c) => {
    const from = info(c.library);
    const keepTo = c.keep_to;
    // An Incoming copy is added where it goes by default; by an administrator also to a main library or their own music
    // (decision 0101 point 2). A kept one moves between an administrator's libraries, never into or out of own music:
    // that would be a copy (the owner, 2026-11-04), which comes with records and copies.
    const targets = !c.deletable
      ? []
      : from?.kind === 'incoming'
        ? others.filter((l) => c.keepable && (l.id === keepTo || (admin && (l.kind === 'main' || l.mine))))
        : admin && from?.kind !== 'own'
          ? others.filter((l) => l.kind !== 'own')
          : [];
    const kept = from?.kind === 'main' || from?.kind === 'own';
    return {
      library: c.library,
      from,
      name: from?.name ?? c.library ?? 'Incoming',
      targets: [...targets.filter((l) => l.id === keepTo), ...targets.filter((l) => l.id !== keepTo)],
      replaceInto: c.deletable && !kept ? replacing : [],
      deletable: c.deletable,
      job: jobOf(c.library),
    };
  });
}

/** An entry of a record's menu that takes a copy somewhere. */
export type CopyEntry = { key: string; offer: AlbumOffer; to: LibraryInfo; label: string };

/** Two copies that would say the same each name themselves. */
const toldApart = (entries: CopyEntry[]) => entries.map((e) => (entries.some((x) => x !== e && x.label === e.label) ? { ...e, label: `${e.label} from ${e.offer.name}` } : e));
const free = (offers: AlbumOffer[]) => offers.filter((o) => o.job === null);
const entry = (offer: AlbumOffer, to: LibraryInfo, label: string): CopyEntry => ({ key: `${offer.library}>${to.id}`, offer, to, label });

/** Add to and Move to, each copy's targets in turn. */
export const moveEntries = (offers: AlbumOffer[]) => toldApart(free(offers).flatMap((o) => o.targets.map((to) => entry(o, to, `${adds(o.from, to) ? 'Add' : 'Move'} to ${to.name}`))));

/** Replace in, one per main library it can take a copy's place in. */
export const replaceEntries = (offers: AlbumOffer[]) => toldApart(free(offers).flatMap((o) => o.replaceInto.map((to) => entry(o, to, `Replace in ${to.name}`))));

/** Delete: a record of several copies names the one that goes, even where the viewer may delete only one of them, as
 *  "Delete EP" would read as the whole record. */
export const deleteEntries = (offers: AlbumOffer[], word: string) =>
  free(offers)
    .filter((o) => o.deletable)
    .map((offer) => ({ offer, label: offers.length > 1 ? `Delete from ${offer.name}` : `Delete ${word}` }));

/** A ticked record with what each of its copies allows. */
type Ticked = { offers: AlbumOffer[] };

/** Several ticked records' Delete, what one has several have. While none of them has two copies, one entry: each its one
 *  copy. Else one per library by its name (`name`), as Move's are, so what goes is said and a record of two copies never
 *  loses both. */
export function deleteGroups<T extends Ticked>(items: T[]) {
  const able = items.flatMap((item) => free(item.offers).flatMap((offer) => (offer.deletable ? { item, offer } : [])));
  if (!able.length) return [];
  if (!items.some((x) => x.offers.length > 1)) return [{ name: null as string | null, them: able }];
  return [...new Set(able.map((x) => x.offer.name))].map((name) => ({ name: name as string | null, them: able.filter((x) => x.offer.name === name) }));
}

/** Each library they can move into, by its name (whichever server's), with what would go. A record goes once: the first
 *  of its copies that can, in the order they would play, as one entry cannot say which. */
export function moveGroups<T extends Ticked>(items: T[]) {
  const by = new Map<string, { item: T; offer: AlbumOffer; to: LibraryInfo; add: boolean }[]>();
  for (const item of items)
    for (const offer of free(item.offers))
      for (const to of offer.targets) {
        const them = by.get(to.name) ?? [];
        if (!them.some((x) => x.item === item)) by.set(to.name, [...them, { item, offer, to, add: adds(offer.from, to) }]);
      }
  // Added where every one of them is (`adds`), else moved.
  return [...by].map(([name, them]) => ({ name, them, adding: them.every((x) => x.add) }));
}

type Copied = Pick<ListedAlbum, 'library' | 'copies' | 'format' | 'bitrate_kbps'>;

/** A record as these copies leave it: the card's facts are the one that plays, the next in the list's order if that went. */
function led<A extends Copied>(a: A, copies: AlbumCopy[]): A {
  const lead = copies.find((c) => c.plays) ?? copies[0];
  return { ...a, copies: copies.map((c) => ({ ...c, plays: c === lead })), library: lead.library, format: lead.format, bitrate_kbps: lead.kbps };
}

/** A record once its copy in `library` went (of two folders the first, as `from` names it): null with its last. A record
 *  stays while one copy does. An album of a list naming no copies is its one library's. */
export function withoutCopy<A extends Copied>(a: A, library?: string): A | null {
  if (!a.copies) return null;
  const at = a.copies.findIndex((c) => c.library === library);
  const rest = a.copies.filter((_, i) => i !== at);
  return rest.length ? led(a, rest) : null;
}

/** A record once its copy in `from` moved into `to`: nothing leaves, the copy lies elsewhere. */
export function withCopyMoved<A extends Copied>(a: A, from: string | undefined, to: string): A {
  if (!a.copies) return { ...a, library: to };
  const at = a.copies.findIndex((c) => c.library === from);
  return led(a, a.copies.map((c, i) => (i === at ? { ...c, library: to } : c)));
}

/** A record's card as the switches stand now: its format and bitrate are the copy that plays, and a switch thrown on the
 *  page moves that with no new list. */
export function heardAs<A extends Copied>(a: A, info: LibraryInfo[] | undefined, on: On): A {
  const c = (a.copies?.length ?? 0) > 1 ? copyPlaying(a, info, on) : null;
  return c && (c.format !== a.format || c.kbps !== a.bitrate_kbps) ? { ...a, format: c.format, bitrate_kbps: c.kbps } : a;
}

/** What "Plays from" offers: a copy for each library on, an Incoming one too, as a pick is how that is heard. Nothing where
 *  fewer than two could be: there is no choice to make, and a pick in a library off would never play. */
export function copiesToHear(a: Held, on: On): AlbumCopy[] {
  const each = (a.copies ?? []).filter((c, i, all) => on(c.library) && all.findIndex((x) => x.library === c.library) === i);
  return each.length > 1 ? each : [];
}

/** A record once the viewer picked its copy in `library`: that copy goes first, as `copy_for` puts a pick, and the card
 *  says so. There is no taking a pick back, only another pick. */
export function withCopyPicked<A extends Copied>(a: A, library: string): A {
  const chosen = a.copies?.find((c) => c.library === library);
  if (!chosen) return a;
  const others = a.copies!.filter((c) => c !== chosen).map((c) => ({ ...c, picked: false, plays: false }));
  return led(a, [{ ...chosen, picked: true, plays: true }, ...others]);
}
