// A record and its copies, the page's rules, pure (docs/records-and-copies.md, "On the page"): whether a record shows,
// the dots on its card, which copy plays, the lines in its panel, and what each copy allows. `on` says whether a library is switched on (lib/sources: `shownIn`), the
// switches being the page's: one changed reaches the server later, and the list is not read again for it.
//
// A list says `copies`, or until the server folds its records (and for a server of another kind) the one `library`, or
// neither: a list with no libraries (the device's, a server's in its mode, Discover).
import type { AlbumCopy, AlbumsSource, LibraryInfo, ListedAlbum } from './api';
import { size } from './exportChoices';

/** Whether a format loses nothing: here so a copy's line can say it, and the list's order by quality too (lib/pageView). */
const LOSSLESS = new Set(['FLAC', 'ALAC', 'WAV', 'AIFF', 'APE', 'WV', 'WAVPACK', 'DSF', 'DFF']);
export const isLossless = (format: string | null | undefined) => !!format && LOSSLESS.has(format.toUpperCase());

export type Held = { library?: string; copies?: AlbumCopy[]; tracks?: number };
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

/** The dots on a record's card, in the list's order: one for each library foreign to home (the owner, 2026-11-06). Home
 *  is the first kept library in the libraries' order, main or own music alike, and a switch does not change what it
 *  means. It goes without saying, so it has a dot only beside another kept library, and a duplicate shows both. Any other
 *  kept library has its dot, as a record only there says where it lies, and so has every Incoming one, beside a kept copy
 *  too. Copies in libraries switched off say nothing while another is on, and a card reached with all of them off (Home's
 *  crates, a search) names where it lies, home too. A server of one library has none: nothing there is foreign. */
export function dotsOf(a: Held, info: LibraryInfo[] | undefined, on: On): Dot[] {
  if (!info || info.length < 2) return [];
  const held = librariesOf(a);
  const allOff = !held.some(on);
  const shown = (allOff ? held : held.filter(on)).flatMap((id) => info.find((l) => l.id === id) ?? []);
  const home = info.find((l) => l.kind !== 'incoming')?.id;
  const keptElsewhere = shown.some((l) => l.kind !== 'incoming' && l.id !== home);
  return shown.flatMap(({ id, colour, name }) => (colour && (id !== home || keptElsewhere || allOff) ? { id, colour, name } : []));
}

/** The copy that plays, as the switches stand now. The server decides (`copy_for`) and lists the copies in its order
 *  for this viewer, off or not, so the first one on is it. One rule is the page's: nobody has checked an Incoming copy,
 *  so beside a kept one it never plays by itself, and it counts only where the server says it plays (a record only in
 *  Incoming). Null: none can, or the list names no copies, and the record's own `format` and `bitrate_kbps` are all
 *  there is. */
export function copyPlaying(a: Held, info: LibraryInfo[] | undefined, on: On): AlbumCopy | null {
  const incoming = (c: AlbumCopy) => info?.find((l) => l.id === c.library)?.kind === 'incoming';
  return a.copies?.find((c) => on(c.library) && (c.plays || !incoming(c))) ?? null;
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

/** A record of several copies names the copy each entry acts on (the owner, testing 0.16.0: on Drift, Music's MP3 and Your music's
 *  FLAC, "Move to lidarr-music" moved Music's copy where the FLAC was meant): every one of them, not only where two would read the
 *  same. Of one copy there is nothing to tell apart. */
const named = (offers: AlbumOffer[], entries: CopyEntry[]) => (offers.length > 1 ? entries.map((e) => ({ ...e, label: `${e.label} from ${e.offer.name}` })) : entries);
const free = (offers: AlbumOffer[]) => offers.filter((o) => o.job === null);
const entry = (offer: AlbumOffer, to: LibraryInfo, label: string): CopyEntry => ({ key: `${offer.library}>${to.id}`, offer, to, label });

/** Add to and Move to, each copy's targets in turn. */
export const moveEntries = (offers: AlbumOffer[]) => named(offers, free(offers).flatMap((o) => o.targets.map((to) => entry(o, to, `${adds(o.from, to) ? 'Add' : 'Move'} to ${to.name}`))));

/** Replace in, one per main library it can take a copy's place in. */
export const replaceEntries = (offers: AlbumOffer[]) => named(offers, free(offers).flatMap((o) => o.replaceInto.map((to) => entry(o, to, `Replace in ${to.name}`))));

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

/** A record's "Play from" entries: each other copy it can be heard from once, by its library's name. An Incoming copy
 *  counts, as this is how one waiting is heard; two folders of one library are one entry, as `from` names a library.
 *  Not a copy in a library off, which a stream never plays, nor the copy that plays, whose entry would only be Play
 *  again. None for a record of one copy, nor for a list that names no copies. */
export function playFromEntries(a: Held, info: LibraryInfo[] | undefined, on: On) {
  const held = new Set(a.copies?.map((c) => c.library));
  if (held.size < 2) return [];
  const playing = copyPlaying(a, info, on)?.library;
  return [...held].filter((id) => on(id) && id !== playing).map((library) => ({ library, label: `Play from ${info?.find((l) => l.id === library)?.name ?? library}` }));
}

/** Whether the viewer may change what a record says (lib/RecordEdit): an administrator, or where it lies only in people's own
 *  music, the owner of the copy added first, which speaks for it. The server's rule is `copies::owned_by`; this one says what to
 *  offer. A copy of one's own of a record in a main library gives no say. */
export function mayEdit(a: Held, info: LibraryInfo[] | undefined, admin: boolean): boolean {
  if (admin) return true;
  const copies = a.copies ?? [];
  const of = (c: AlbumCopy) => info?.find((l) => l.id === c.library);
  if (!copies.length || !copies.every((c) => of(c)?.kind === 'own')) return false;
  const first = copies.reduce((x, y) => ((y.added ?? Infinity) < (x.added ?? Infinity) ? y : x));
  return !!of(first)?.mine;
}

/** One copy's line in its record's panel. */
export type CopyLine = {
  name: string;
  /** Its library's dot. Every line has it, whatever a card's dots leave out: a line goes by its library. */
  colour: string | null;
  /** "MP3 320", "FLAC". */
  quality: string;
  /** "92 MB"; null where the list says none. A copy short of the record's tracks, still arriving or for good: "3 of 10", as how
   *  much of it is there says more than its bytes. */
  size: string | null;
  /** It is what Play would give. */
  plays: boolean;
  /** A tap's Play from, the menu's own entry for this copy: null where the menu has none. */
  playFrom: ReturnType<typeof playFromEntries>[number] | null;
};

/** A lossy copy's bitrate tells it from another; a lossless one's says nothing of it. A copy with no one format is as a
 *  card says it. */
const qualityOfCopy = (c: AlbumCopy) => (c.format ? [c.format, isLossless(c.format) ? null : c.kbps].filter(Boolean).join(' ') : 'Mixed formats');

/** A record's copies, a line each in its panel, so that what tells them apart is said of each (the owner, 2026-11-06:
 *  "What's the difference!?!"): in the list's order, so the order they would play. None for a record of one copy, whose
 *  details say it, nor for a list that names no copies. A copy in a library off has no line, as it cannot be heard. The
 *  one left on by that keeps its line: the record's own quality is the server's, which can be the copy off. Two folders
 *  of one library are a line each, as they can differ, and the later one has no tap: `from` names the first. */
export function copyLines(a: Held, info: LibraryInfo[] | undefined, on: On): CopyLine[] {
  const copies = a.copies ?? [];
  if (copies.length < 2) return [];
  const playing = copyPlaying(a, info, on);
  const entries = playFromEntries(a, info, on);
  return copies.flatMap((c, i) => {
    if (!on(c.library)) return [];
    const library = info?.find((l) => l.id === c.library);
    const first = copies.findIndex((x) => x.library === c.library) === i;
    return {
      name: library?.name ?? c.library,
      colour: library?.colour ?? null,
      quality: qualityOfCopy(c),
      size: a.tracks && c.tracks < a.tracks ? `${c.tracks} of ${a.tracks}` : c.size ? size(c.size) : null,
      plays: c === playing,
      playFrom: first ? (entries.find((e) => e.library === c.library) ?? null) : null,
    };
  });
}
