// Quick crates (0.11; the owner, 2026-10-29 and 2026-10-30): a crate made from a few typed terms, each a chip. A
// genre, style, mood, tag, label, album or year (or decade) narrows it; an artist means "sounds like" them, the chip
// switching to "by" them, their own tracks left out unless kept. Filtered first, then ranked by how alike, cut where
// it stops being close (the closeness curve's handle loosens it). A word that reads several ways takes its best
// reading, its kind shown, the others a tap away. Pure: terms in, chips and a crate's conditions out.
import type { Condition } from './api';
import { byRelevance, relevance, words, type SearchResult } from './search';

export type ChipKind = 'artist' | 'album' | 'track' | 'label' | 'genre' | 'style' | 'mood' | 'tag' | 'year';
/** One way a term reads: its kind, what a condition matches (a name, an album's id, a year's span `min-max`), and how
 *  it shows. An artist's `id` (the library's `ar-…`) seeds "sounds like". */
export type Reading = { kind: ChipKind; value: string; label: string; id?: string };
/** A typed term made a chip: its reading, the other ways it could read, and for an artist "like" or "by" them and
 *  whether their own tracks stay. */
export type Chip = { term: string; reading: Reading; others: Reading[]; how?: 'like' | 'by'; own?: boolean; not?: boolean };

/** Where the cut starts: a close fit (0–100, the closeness curve's scale; the owner: "Close, adjustable"). Provisional
 *  until measured on a library. */
export const CLOSE = 60;
/** How many the closest may be at most, the cut deciding below it (the server's own limit). */
export const LIKE_MOST = 1000;

/** The kinds whose several terms join "and" or "or" (0.13, decision 0088; the owner, 2026-10-31): Sounds like (and: one
 *  profile blended from them all, the default; or: near any of them) and the kinds a track carries several of, genres,
 *  styles, moods and tags (or: any of them, the default; and: all of them). A year, a label, an album: one a track, so
 *  no choice. */
export type JoinKind = 'like' | 'genre' | 'style' | 'mood' | 'tag';
export type Join = 'and' | 'or';
/** How each kind's terms join where it differs from the default (`joinOf`). */
export type Modes = Partial<Record<JoinKind, Join>>;
const MULTI: readonly JoinKind[] = ['genre', 'style', 'mood', 'tag'];
const isMulti = (k: string): k is JoinKind => (MULTI as readonly string[]).includes(k);
/** A kind's join: as set, else Sounds like "and" (blended), the others "or" (any of them). */
export const joinOf = (modes: Modes, kind: JoinKind): Join => modes[kind] ?? (kind === 'like' ? 'and' : 'or');
/** The other join (a tap on the sentence's "and" or "or"). */
export const flipped = (j: Join): Join => (j === 'and' ? 'or' : 'and');

/** The kinds search is asked for, a term's readings among them. */
export const TERM_KINDS = ['artist', 'album', 'label', 'genre', 'style', 'mood', 'tag'] as const;
/** Equally good readings in this order: what narrows broadly before a name (the owner's example: "Soul" a genre
 *  before an artist of that name). */
const ORDER: ChipKind[] = ['year', 'genre', 'style', 'mood', 'artist', 'label', 'album', 'tag'];

/** The terms typed: apart at commas, semicolons and line breaks, each trimmed, a term typed twice once. */
export function splitTerms(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of text.split(/[,;\n]+/)) {
    const term = t.trim().replace(/\s+/g, ' ');
    const key = term.toLowerCase();
    if (!term || seen.has(key)) continue;
    seen.add(key);
    out.push(term);
  }
  return out;
}

/** What search found for a phrase: its hits of the term kinds (a few of each), as a phrase's readings come from. */
export type Found = readonly (SearchResult & { id?: string })[];
/** Asks search for a phrase (the sheet's: the term kinds, a few of each). */
export type Ask = (phrase: string) => Promise<Found>;
/** A phrase read off a line: the words it took, and their readings, the best first (none: nothing found); `how` an
 *  artist (or an album) asked for as "like" or "by" by the sentence's own words before it ("sounds like", "by"). */
export type Read = { term: string; readings: Reading[]; how?: 'like' | 'by'; join?: Join; not?: boolean };

/** What the sentence's own words ask of the terms after them (0.12.x; the owner, 2026-10-31: "and" did not work): a
 *  kind ("on" a label, "tagged" a tag…), or an artist "like" or "by". */
type Want = { kind?: ChipKind; how?: 'like' | 'by'; not?: boolean };
/** The words the sentence joins its terms with (sentenceOf's, and what people type between them): dropped between
 *  terms, never where a name holds them ("The Roots", "Earth, Wind & Fire", "The Dark Side of the Moon"). */
const JOINS = new Set(['and', 'or', '&', 'the', 'from', 'with', 'in', 'of', 'that', 'to', 'tracks', 'a']);
/** The sentence's kind words, longest first: what follows them reads as that kind where it can. */
const MARKERS: [string[], Want][] = (
  [
    [['that', 'sounds', 'like'], { how: 'like' }],
    [['that', 'sound', 'like'], { how: 'like' }],
    [['sounds', 'like'], { how: 'like' }],
    [['sound', 'like'], { how: 'like' }],
    [['like'], { how: 'like' }],
    [['that', 'feels'], { kind: 'mood' }],
    [['that', 'feel'], { kind: 'mood' }],
    [['feels'], { kind: 'mood' }],
    [['feel'], { kind: 'mood' }],
    [['from', 'the', 'album'], { kind: 'album' }],
    [['the', 'album'], { kind: 'album' }],
    [['album'], { kind: 'album' }],
    [['tagged'], { kind: 'tag' }],
    [['style'], { kind: 'style' }],
    [['by'], { how: 'by' }],
    [['on'], { kind: 'label' }],
    // "But not" (0.13): what follows is left out, through a kind word after it ("but not on Def Jam").
    [['but', 'not'], { not: true }],
    [['not'], { not: true }],
    // Said otherwise (the owner, 2026-10-31: "any other missing completions?"): "without", "except", "excluding", "no".
    [['but', 'without'], { not: true }],
    [['without'], { not: true }],
    [['except'], { not: true }],
    [['excluding'], { not: true }],
    [['no'], { not: true }],
  ] as [string[], Want][]
).sort((a, b) => b[0].length - a[0].length);
const isJoin = (t: string) => JOINS.has(t.toLowerCase());

/** The words a typed sentence joins its terms with: the join words and the kind words ("and", "from the", "like", "by",
 *  "but not"…). */
const SENTENCE_WORDS = new Set([...JOINS, ...MARKERS.flatMap(([ws]) => ws)]);
/** A typed word as compared: its accents and case gone, the punctuation around it too ("Davis," is "davis"). */
const foldWord = (w: string) =>
  w
    .normalize('NFKD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/^[^\p{L}\p{N}&]+|[^\p{L}\p{N}&]+$/gu, '');

/** Where the glass's field dims what it holds (0.13; the owner, 2026-10-31: the typed text as typed, only its joining
 *  words lighter, as the sentence's, once the words read as a crate): the joining words typed between its terms, each
 *  run one range [start, end) of `text`. A term's own words are never dimmed ("The Roots", "Earth, Wind & Fire"): the
 *  terms, in the order typed, are found first. */
export function dimmedIn(text: string, terms: readonly string[]): [number, number][] {
  const tokens = [...text.matchAll(/\S+/g)].map((m) => ({ start: m.index, end: m.index + m[0].length, w: foldWord(m[0]) }));
  const inTerm = new Array<boolean>(tokens.length).fill(false);
  const at = (j: number, ws: string[]) => ws.every((w, k) => tokens[j + k]?.w === w && !inTerm[j + k]);
  let from = 0;
  for (const term of terms) {
    const ws = term.split(/\s+/).map(foldWord).filter(Boolean);
    if (!ws.length) continue;
    let j = tokens.findIndex((_, k) => k >= from && at(k, ws));
    if (j < 0) j = tokens.findIndex((_, k) => at(k, ws));
    if (j < 0) continue;
    ws.forEach((_, k) => (inTerm[j + k] = true));
    from = j + ws.length;
  }
  const out: [number, number][] = [];
  tokens.forEach((t, k) => {
    if (inTerm[k] || !SENTENCE_WORDS.has(t.w)) return;
    const last = out[out.length - 1];
    // A run ("from the", "that sounds like") one range, the spaces in it too.
    if (last && k > 0 && last[1] === tokens[k - 1].end && !inTerm[k - 1]) last[1] = t.end;
    else out.push([t.start, t.end]);
  });
  return out;
}
/** A phrase without its join words ("earth wind and fire" → "earth wind fire", as "Earth, Wind & Fire" reads). */
const stripped = (phrase: string) =>
  phrase
    .split(' ')
    .filter((t) => !isJoin(t))
    .join(' ');

/** The longest phrase tried past a line's whole run (the owner, 2026-10-30: "Read the whole line"): names of up to
 *  three words read together, a longer one where the whole run is it ("the dark side of the moon"). */
export const MOST_WORDS = 3;
/** A phrase fits a hit when the hit's own name holds its words and they make up this much of the name (a few letters
 *  still to type, or "hiphop" for Hip Hop): "miles davis" Miles Davis, not "miles" Miles Davis Quintet. */
export const FIT = 0.7;

/** Whether one of a phrase's hits is the phrase: its own name holds the words, the words most of the name. A guess
 *  (the matcher's fuzzy: typos forgiven) fits only a phrase about as long as the name ("miles daivs" Miles Davis), so
 *  a word too many is read apart, not swallowed ("jazz funk soul" is not Jazz-Funk). */
export function fits(phrase: string, found: Found): boolean {
  const q = words(phrase);
  const typed = q.join('').length;
  return found.some((r) => {
    if (!readingOf(r)) return false;
    const rel = relevance(q, r);
    const guess = rel[1] < 0;
    return rel[0] === 1 && rel[5] >= FIT && (!guess || typed <= words(r.title).join('').length / FIT);
  });
}

/** A line read as the owner types it (2026-10-30: "hip hop 90s miles davis"): apart at commas and semicolons
 *  (`splitTerms`); in each piece the years and decades first, by rule ("1970 to 1985" one span); the words between
 *  them read longest first, the whole run, then three words, two, one, a phrase taken where one of its hits fits it
 *  (`fits`, a name's "&" an "and"); where none fits, the one word, read by its best hit as a term always was. The
 *  sentence's own words (0.12.x; the owner, 2026-10-31: "and" did not work) are read as it writes them: a join
 *  ("and", "or", "the", "from"…) dropped where no name holds it; a kind word ("sounds like", "by", "on", "tagged",
 *  "feels", "from the album", "style") making what follows that kind where it reads so, until the next; so whatever
 *  sentenceOf writes, typed back, reads to the same terms. Each phrase asked once; the shorter ones at a place
 *  together, after the longest did not fit. In the order typed. */
export async function readLine(text: string, ask: Ask, now?: number): Promise<Read[]> {
  const asked = new Map<string, Promise<Found>>();
  const once = (phrase: string) => {
    const key = words(phrase).join(' ');
    let a = asked.get(key);
    if (!a) asked.set(key, (a = ask(phrase).catch(() => [] as Found)));
    return a;
  };
  // A phrase's finds where they fit it, as typed or without its join words (asked so too, for "Earth, Wind & Fire").
  const fitting = async (phrase: string): Promise<Found | null> => {
    const found = await once(phrase);
    if (fits(phrase, found)) return found;
    const bare = stripped(phrase);
    if (bare && bare !== phrase && words(bare).length) {
      const again = await once(bare);
      if (fits(bare, again)) return again;
    }
    return null;
  };
  // The term read, the kind its sentence's words asked for first where one of its finds is that and fits it.
  const read = (term: string, found: Found, want: Want | null): Read => {
    let readings = readingsFor(term, found, now);
    const not = want?.not ? { not: true } : {};
    if (want && (want.kind || want.how)) {
      const kinds: ChipKind[] = want.kind ? [want.kind] : want.how === 'like' ? ['artist', 'album'] : ['artist'];
      const bare = stripped(term) || term;
      const first = found
        .filter((h) => {
          const r = readingOf(h);
          return !!r && kinds.includes(r.kind) && (fits(term, [h]) || fits(bare, [h]));
        })
        .map((h) => readingOf(h)!);
      const key = (r: Reading) => `${r.kind}\n${r.value}`;
      if (first.length) readings = [...first, ...readings.filter((r) => !first.some((f) => key(f) === key(r)))];
      if (want.how && readings[0] && kinds.includes(readings[0].kind)) return { term, readings, how: want.how, ...not };
    }
    return { term, readings, ...not };
  };
  const markerAt = (tokens: string[], i: number) =>
    MARKERS.find(([ws]) => ws.every((w, k) => tokens[i + k]?.toLowerCase() === w)) ?? null;

  // One run of words between years: its terms, and the kind asked for at its end (it carries over a comma: "sounds
  // like Bonobo, Floating Points and …").
  async function segment(tokens: string[], carried: Want | null): Promise<{ reads: Read[]; want: Want | null }> {
    const out: Read[] = [];
    let want: Want | null = carried;
    // The "and" or "or" dropped before the next term (0.13: it sets how that kind's terms join, modesFrom).
    let join: Join | undefined;
    const push = (r: Read) => {
      out.push(join ? { ...r, join } : r);
      join = undefined;
    };
    for (let i = 0; i < tokens.length; ) {
      const phrase = (n: number) => tokens.slice(i, i + n).join(' ');
      const marker = markerAt(tokens, i);
      if (marker) {
        // A name that begins with the word ("Like a Rolling Stone", "On the Corner") stays the name.
        const next = tokens.slice(i).findIndex((_, k) => k > 0 && markerAt(tokens, i + k));
        const whole = next < 0 ? tokens.length - i : next;
        // Its own name holds the word: the matcher forgives a word too many ("by al green" for Al Green), so that's no name.
        const found = whole > marker[0].length ? await fitting(phrase(whole)) : null;
        const name = found?.filter((h) => words(h.title).includes(marker[0][0]) && fits(phrase(whole), [h])) ?? [];
        if (name.length) {
          push(read(phrase(whole), name, null));
          i += whole;
          continue;
        }
        const m = marker[1];
        want = m.not ? { not: true } : m.how === 'like' || !want?.not ? m : { ...m, not: true };
        i += marker[0].length;
        continue;
      }
      const room = tokens.length - i;
      const longest = i === 0 ? room : Math.min(MOST_WORDS, room);
      // A phrase that may be a name: not begun or ended by a join word unless that is how the name reads as typed.
      const tryAt = async (n: number): Promise<Found | null> => {
        const p = phrase(n);
        const edges = [tokens[i], tokens[i + n - 1]].filter(isJoin);
        // The sentence's kind words inside it ("hip hop not", "rock by"), each only where the name holds it ("Stand by
        // Me"): else "hip hop not 00s" read Hip Hop, its "not" lost, and the 2000s went in (the owner, 2026-10-31).
        const inner = Array.from({ length: n - 1 }, (_, k) => i + k + 1)
          .filter((j) => markerAt(tokens, j))
          .map((j) => tokens[j].toLowerCase());
        if (!edges.length && !inner.length) return fitting(p);
        // A join word at its edge only where the name holds it ("The Roots"): the matcher forgives a word too many, so
        // "Miles Davis or" would otherwise read as Miles Davis, its "or" lost (0.13: that "or" is the crate's choice).
        const f = (await once(p)).filter((h) => {
          const own = words(h.title);
          return (
            edges.every((e) => own.includes(e.toLowerCase()) || ((e === '&' || e.toLowerCase() === 'and') && h.title.includes('&'))) &&
            inner.every((w) => own.includes(w))
          );
        });
        return fits(p, f) ? f : null;
      };
      let took = 0;
      let found: Found = [];
      const whole = await tryAt(longest);
      if (whole) [took, found] = [longest, whole];
      else if (longest > 1) {
        // The shorter ones at this place together, the longest that fits taken.
        const shorter = Array.from({ length: Math.min(MOST_WORDS, longest - 1) }, (_, k) => Math.min(MOST_WORDS, longest - 1) - k);
        const tries = await Promise.all(shorter.map(async (n) => ({ n, f: await tryAt(n) })));
        const fit = tries.find((x) => x.f);
        if (fit) [took, found] = [fit.n, fit.f!];
      }
      if (!took) {
        // No name here: a join word goes, any other word is a term of its own, read by its best find.
        if (isJoin(tokens[i])) {
          const w = tokens[i].toLowerCase();
          if (w === 'and' || w === '&' || w === 'or') join = w === 'or' ? 'or' : 'and';
          i += 1;
          continue;
        }
        took = 1;
        found = await once(phrase(1));
      }
      push(read(phrase(took), found, want));
      i += took;
    }
    return { reads: out, want };
  }

  const reads: Read[] = [];
  let carried: Want | null = null;
  for (const piece of splitTerms(text)) {
    const tokens = piece
      .replace(/(\d{4})\s*(?:[-–]|\bto\b)\s*(\d{4})/gi, '$1-$2')
      // "before 1970", "after the 90s", "since 2010", "until 1980": one open span (yearOf), read as one term.
      .replace(OPEN_YEARS, (_, w: string, y: string) => `${w.toLowerCase()}:${y}`)
      .split(' ');
    const parts: (Read | string[])[] = [];
    for (const t of tokens) {
      if (!words(t).length && t !== '&') continue;
      const year = yearOf(t, now);
      if (year) parts.push({ term: t.replace(':', ' '), readings: [year] });
      else if (Array.isArray(parts.at(-1))) (parts.at(-1) as string[]).push(t);
      else parts.push([t]);
    }
    let want: Want | null = carried;
    let wanted = false;
    for (const p of parts) {
      if (!Array.isArray(p)) {
        // A year after "not" left out ("hip hop not 00s"), the "not" on to the next kind word, as between words.
        reads.push(want?.not ? { ...p, not: true } : p);
        if (!want?.not) want = null;
        continue;
      }
      const s = await segment(p, want);
      reads.push(...s.reads);
      want = s.want;
      wanted = !!s.want && s.reads.length > 0;
    }
    // A kind carried over a comma only where the piece ended on a term of it.
    carried = wanted ? want : null;
  }
  const seen = new Set<string>();
  return reads.filter((r) => {
    const key = r.term.toLowerCase();
    return !seen.has(key) && (seen.add(key), true);
  });
}

/** The words that open a span on one side, with the year or decade after them ("before the 70s"): readLine makes them
 *  one token ("before:70s"), yearOf reads it. */
const OPEN_YEARS = /\b(before|after|since|until|till)\s+(?:the\s+)?([’‘']?\d{2,4}[’‘']?s?)(?=\s|$)/gi;
/** An open span's missing end, as a year reading's value holds it ("0-1969": before 1970; "2001-9999": after 2000). */
export const NO_START = 0;
export const NO_END = 9999;
/** The decades said in words ("the seventies"). */
const DECADE_WORDS: Record<string, number> = { twenties: 20, thirties: 30, forties: 40, fifties: 50, sixties: 60, seventies: 70, eighties: 80, nineties: 90, noughties: 0, aughts: 0 };

/** A year or a decade as typed: 1975, '75, 1970s, '70s, 70s, seventies, 1970-1979, 70s-80s; an open one after
 *  readLine's "before", "after", "since", "until" ("before:1970"); `now` for which century a two-digit year is (one
 *  after this year's two digits is last century's). */
export function yearOf(term: string, now = new Date().getFullYear()): Reading | null {
  const t = term.trim().toLowerCase().replace(/[’‘]/g, "'");
  const two = (yy: number) => (yy <= now % 100 ? 2000 + yy : 1900 + yy);
  const span = (min: number, max: number, label: string): Reading => ({ kind: 'year', value: `${min}-${max}`, label });
  let m = /^(before|after|since|until|till):(.+)$/.exec(t);
  if (m) {
    const inner = yearOf(m[2], now);
    if (!inner) return null;
    const [a, b] = inner.value.split('-').map(Number);
    const said = `${m[1] === 'till' ? 'until' : m[1]} ${/^\d{3}0s$/.test(inner.label) ? `the ${inner.label}` : inner.label}`;
    if (m[1] === 'before') return span(NO_START, a - 1, said);
    if (m[1] === 'after') return span(b + 1, NO_END, said);
    if (m[1] === 'since') return span(a, NO_END, said);
    return span(NO_START, b, said);
  }
  if (t in DECADE_WORDS) {
    const d = two(DECADE_WORDS[t]);
    return span(d, d + 9, `${d}s`);
  }
  // Decade to decade: "70s-80s", "1970s-80s", "1970s–1980s".
  m = /^'?(\d{1,3})0'?s\s*[-–]\s*'?(\d{1,3})0'?s$/.exec(t);
  if (m) {
    const dec = (d: string) => (d.length === 3 ? Number(d) * 10 : two(Number(d) * 10));
    const [a, b] = [dec(m[1]), dec(m[2])];
    if (b >= a) return span(a, b + 9, `${a}s to ${b}s`);
  }
  m = /^(\d{4})\s*[-–]\s*(\d{4})$/.exec(t);
  if (m) {
    const [a, b] = [Number(m[1]), Number(m[2])];
    return span(Math.min(a, b), Math.max(a, b), `${Math.min(a, b)}–${Math.max(a, b)}`);
  }
  m = /^(\d{3})0'?s$/.exec(t);
  if (m) return span(Number(m[1]) * 10, Number(m[1]) * 10 + 9, `${m[1]}0s`);
  m = /^'?(\d)0'?s$/.exec(t);
  if (m) {
    const d = two(Number(m[1]) * 10);
    return span(d, d + 9, `${d}s`);
  }
  m = /^(\d{4})$/.exec(t);
  if (m && Number(m[1]) >= 1000) return span(Number(m[1]), Number(m[1]), m[1]);
  m = /^'(\d{2})$/.exec(t);
  if (m) {
    const y = two(Number(m[1]));
    return span(y, y, String(y));
  }
  return null;
}

/** A search result as a reading, where it is one (a track, a playlist or a crate is not). */
export function readingOf(r: SearchResult & { id?: string }): Reading | null {
  const k = r.kind;
  if (k === 'artist') return { kind: 'artist', value: r.title, label: r.title, ...(r.id ? { id: r.id } : {}) };
  if (k === 'album') {
    const id = /^album:[^/]+\/(.+)$/.exec(r.subject)?.[1];
    return id ? { kind: 'album', value: id, label: r.title } : null;
  }
  if (k === 'label' || k === 'genre' || k === 'style' || k === 'mood' || k === 'tag') return { kind: k, value: r.title, label: r.title };
  return null;
}

/** A term's readings, the best first: a year or decade where it is one, then search's finds by how well they match the
 *  term (its words, the name's own first), equals in ORDER; each once. */
export function readingsFor(term: string, found: readonly (SearchResult & { id?: string })[], now?: number): Reading[] {
  const q = words(term);
  const year = yearOf(term, now);
  const ranked = found
    .map((r) => ({ read: readingOf(r), rel: relevance(q, r) }))
    .filter((x): x is { read: Reading; rel: number[] } => x.read !== null)
    .sort((a, b) => byRelevance(a.rel, b.rel) || ORDER.indexOf(a.read.kind) - ORDER.indexOf(b.read.kind));
  const out: Reading[] = year ? [year] : [];
  for (const { read } of ranked) if (!out.some((o) => o.kind === read.kind && o.value === read.value)) out.push(read);
  return out;
}

/** A chip from a term's readings (none: no chip); an artist "sounds like" by default where it can seed (its id), or
 *  as the sentence's words asked (`how`). */
export function chipOf(term: string, readings: readonly Reading[], how?: 'like' | 'by', not?: boolean): Chip | null {
  const [reading, ...others] = readings;
  if (!reading) return null;
  // Left out (0.13, "but not"): a kind a crate can leave out, an artist "by" them, a year or a span its own; a track
  // cannot be (no condition names one).
  if (not) {
    if (reading.kind === 'track') return null;
    return reading.kind === 'artist' ? { term, reading, others, how: 'by', own: false, not: true } : { term, reading, others, not: true };
  }
  // An album asked for as "like" by the sentence's words ("sounds like Kind of Blue") seeds by its id, as a menu's does.
  if (how === 'like' && reading.kind === 'album') return { term, reading: { ...reading, id: reading.value }, others, how, own: false };
  if (reading.kind !== 'artist') return { term, reading, others };
  return { term, reading, others, how: reading.id && how !== 'by' ? 'like' : 'by', own: false };
}

/** The chip read another way (a tap on another reading): the one it read becomes one of the others. */
export function readAs(chip: Chip, reading: Reading): Chip {
  const others = [chip.reading, ...chip.others].filter((r) => !(r.kind === reading.kind && r.value === reading.value));
  const artist = reading.kind === 'artist' ? { how: reading.id ? (chip.how ?? 'like') : ('by' as const), own: chip.own ?? false } : {};
  return { term: chip.term, reading, others, ...artist };
}

/** Whether a term can be left out (0.13, the term's Leave out; the owner, 2026-10-31): what a crate's conditions can
 *  negate as the sentence says it. Not a track (no condition names one); a year or a span is left out on its own,
 *  apart from the years' one span. */
export const canLeaveOut = (chip: Chip) => chip.reading.kind !== 'track';

/** The term left out ("but not …") or taken back in: read again from its own readings, so an artist left out is "by"
 *  them and, back in, sounds like them again where it can. */
export function leftOut(chip: Chip, not: boolean): Chip {
  if (!!chip.not === not || (not && !canLeaveOut(chip))) return chip;
  return chipOf(chip.term, [chip.reading, ...chip.others], undefined, not) ?? chip;
}

/** A chip that seeds "sounds like": an artist read that way, or a track or an album a menu's "Sounds like this" handed
 *  over (0.12; the owner: one way to make a crate), each by its library id. */
const likes = (c: Chip) => c.how === 'like' && !!c.reading.id && !c.not;

/** "Sounds like this" from a menu (0.12): the track, album or artist as the crate's "like" term, by its library id
 *  (`tr-…`, `al-…`, `ar-…`); their own tracks left out, as a typed artist's are. */
export function likeChip(kind: 'track' | 'album' | 'artist', id: string, label: string): Chip {
  return { term: label, reading: { kind, value: kind === 'artist' ? label : id, label, id }, others: [], how: 'like', own: false };
}

/** "Grab a crate" from a genre's, a style's, a mood's, a tag's or a label's own crate (0.12): that word as its term. */
export function wordChip(kind: 'genre' | 'style' | 'mood' | 'tag' | 'label', word: string): Chip {
  return { term: word, reading: { kind, value: word, label: word }, others: [] };
}

/** Global search's words read as a crate (0.12; the owner, 2026-10-30: mockup 1a, the crate card first): its chips
 *  where they read as two terms or more of two kinds or more (a year and an artist; a genre, a decade and an artist),
 *  or one of them left out (the owner, 2026-10-31: "jazz and blues, but not techno" should also work: a "not" is said
 *  only of a crate), or joined by a typed "or" or "and" (the owner, 2026-10-31: "jazz or blues… should also result in
 *  a crate"), null where they are one thing (a name, a genre): no crate then (the owner, 2026-10-31). */
export function crateOfLine(reads: readonly Read[]): Chip[] | null {
  const chips = reads.flatMap((r) => chipOf(r.term, r.readings, r.how, r.not) ?? []);
  const kinds = new Set(chips.map((c) => c.reading.kind));
  const leaves = chips.some((c) => c.not) && chips.some((c) => !c.not);
  const joined = reads.some((r) => r.join && r.readings.length);
  return chips.length >= 2 && (kinds.size >= 2 || leaves || joined) ? chips : null;
}

/** The crate a search's words make, at the top of its results (the owner, 2026-10-31: global search's and every page's
 *  own, one card): only where they read as a crate (crateOfLine: two kinds or more) that holds a track; nothing while
 *  its count is not known, so no empty crate comes and goes. */
export const crateShows = (chips: readonly Chip[] | null, total: number | null | undefined): boolean =>
  !!chips && chips.length > 0 && total != null && total >= 1;

/** The pages whose own search lists music or crates, where the crate card leads its results (the owner, 2026-10-31).
 *  Not Home (its search is everywhere's, which has the card), nor the pages that find settings, insights, health or
 *  MusicBrainz. */
const CRATE_CARD_PAGES = new Set(['library', 'playlists', 'downloads', 'discover', 'requests', 'crates']);
export const crateCardOn = (page: string) => CRATE_CARD_PAGES.has(page);

/** What a page's own list is matched by (0.13; the owner, 2026-10-31: a typed sentence found nothing under its card):
 *  where the search's words read as a crate, each of its terms (a left-out one aside), an item matching any of them,
 *  the joining words never asked; else the whole line, as ever. */
export function pageQueries(q: string, chips: readonly Chip[] | null): string[][] {
  const terms = (chips ?? []).filter((c) => !c.not).map((c) => words(c.term)).filter((w) => w.length);
  return terms.length ? terms : [words(q)];
}

/** A line read onto a crate's terms (the crate search's words, lib/quickCrates): the terms not in it yet as chips, how
 *  an "and" or "or" joins them with what it has (modesFrom), and the phrases that read as nothing. */
export function readOnto(chips: readonly Chip[], read: readonly Read[]): { add: Chip[]; modes: Modes; missed: string[] } {
  const fresh = read.filter((r) => !chips.some((c) => c.term.toLowerCase() === r.term.toLowerCase()));
  const have = chips.flatMap((c) => joinKindOf(c) ?? []);
  return {
    add: fresh.flatMap((r) => chipOf(r.term, r.readings, r.how, r.not) ?? []),
    modes: modesFrom(fresh, have),
    missed: fresh.filter((r) => !r.readings.length).map((r) => r.term),
  };
}

/** The join kind a read term makes, as chipOf will chip it: an artist with an id "like" unless asked "by", an album asked
 *  "like", else its kind where several of it join (none for a year, a label, an album). */
function joinKindOfRead(r: Read): JoinKind | null {
  const k = r.readings[0];
  if (!k) return null;
  if ((k.kind === 'artist' && k.id && r.how !== 'by') || (k.kind === 'album' && r.how === 'like')) return 'like';
  return isMulti(k.kind) ? k.kind : null;
}
/** The kind a chip's terms join as (Sounds like, or a kind several of which a track carries). */
export function joinKindOf(c: Chip): JoinKind | null {
  return likes(c) ? 'like' : isMulti(c.reading.kind) ? c.reading.kind : null;
}

/** How the terms typed join (0.13; the owner: the sentence's own "and" or "or" is the choice): an "and" or "or" typed
 *  between two terms of one kind (with `have`, the kinds already in the crate) sets that kind's join; the last said
 *  wins. */
export function modesFrom(reads: readonly Read[], have: Iterable<JoinKind> = []): Modes {
  const seen = new Set<JoinKind>(have);
  const out: Modes = {};
  for (const r of reads) {
    const k = r.not ? null : joinKindOfRead(r);
    if (!k) continue;
    if (r.join && seen.has(k)) out[k] = r.join;
    seen.add(k);
  }
  return out;
}

/** The seeds of "sounds like": the artists chipped that way. */
export const seedsOf = (chips: readonly Chip[]): string[] => chips.filter(likes).map((c) => c.reading.id!);

/** Whether the artists chipped "like" keep their own tracks (one of them keeping them keeps them). */
export const keepsOwn = (chips: readonly Chip[]) => chips.some((c) => likes(c) && !!c.own);

/** The chips as a crate's conditions: each kind's chips one row, any of them (two genres: either); the years one span
 *  from the earliest to the latest; the artists "by" them one row; those "like" them one Sounds like, cut at `min`,
 *  their own tracks out unless one of them keeps them. */
export function conditionsOf(chips: readonly Chip[], min = CLOSE, modes: Modes = {}): Condition[] {
  const rows: Condition[] = [];
  const byAxis = new Map<string, string[]>();
  const add = (axis: string, v: string) => {
    const vs = byAxis.get(axis) ?? [];
    if (!vs.includes(v)) vs.push(v);
    byAxis.set(axis, vs);
  };
  // What is left out (0.13, "but not"): each kind's one row, negated (none of them).
  const notAxis = new Map<string, string[]>();
  let years: [number, number] | null = null;
  // Years left out: each its own negated span ("not the 2000s, not 1975").
  const notYears: [number, number][] = [];
  for (const c of chips) {
    const r = c.reading;
    if (c.not) {
      if (r.kind === 'track') continue;
      if (r.kind === 'year') {
        const [a, b] = r.value.split('-').map(Number);
        if (!notYears.some(([x, y]) => x === a && y === b)) notYears.push([a, b]);
        continue;
      }
      const vs = notAxis.get(r.kind) ?? [];
      if (!vs.includes(r.value)) vs.push(r.value);
      notAxis.set(r.kind, vs);
      continue;
    }
    if (likes(c) || r.kind === 'track') continue;
    if (r.kind === 'year') {
      const [a, b] = r.value.split('-').map(Number);
      years = years ? [Math.min(years[0], a), Math.max(years[1], b)] : [a, b];
    } else add(r.kind, r.value);
  }
  // Several of a kind a track carries, joined "and": all of them (0.13).
  for (const [axis, values] of byAxis) {
    const all = values.length > 1 && isMulti(axis) && joinOf(modes, axis) === 'and';
    rows.push({ axis, not: false, value: { kind: all ? 'all_of' : 'any_of', values } });
  }
  // An open end ("before 1970") none (the conditions' open range).
  const end = (y: number) => (y <= NO_START || y >= NO_END ? null : y);
  if (years) rows.push({ axis: 'year', not: false, value: { kind: 'range', min: end(years[0]), max: end(years[1]) } });
  for (const [axis, values] of notAxis) rows.push({ axis, not: true, value: { kind: 'any_of', values } });
  for (const [min, max] of notYears) rows.push({ axis: 'year', not: true, value: { kind: 'range', min: end(min), max: end(max) } });
  const seeds = seedsOf(chips);
  if (seeds.length) {
    const either = seeds.length > 1 && joinOf(modes, 'like') === 'or';
    rows.push({
      axis: 'like',
      not: false,
      value: { kind: 'like', of: seeds, count: LIKE_MOST, min, ...(keepsOwn(chips) ? { own: true } : {}), ...(either ? { mode: 'either' as const } : {}) },
    });
  }
  return rows;
}

/** A part of a crate's sentence: a term (its chip's place, a tap reads it another way) or the words between; `tight`:
 *  no space before it (a comma). */
export type Part = { text: string; chip?: number; tight?: boolean; flip?: JoinKind };

/** The crate read as a sentence (0.12; the owner, 2026-10-30: the crate search's mockup B): "Hip Hop from the 1990s that
 *  sounds like Miles Davis". As `conditionsOf` combines them, so it says what the crate is: a kind's terms any of them
 *  ("or"), the kinds all of them; the years one span from the earliest to the latest; the artists "like" one Sounds
 *  like of them all ("and", as the server names it). Genres lead (else styles, else "Tracks"); then a style beside a
 *  genre, tags, by, on a label, from an album, the years; last what it feels and sounds like. */
export function sentenceOf(chips: readonly Chip[], modes: Modes = {}, brief: string | null = null): Part[] {
  if (!chips.length && !brief) return [];
  const all = chips.map((c, i) => ({ c, i }));
  const of = (kind: ChipKind) => all.filter(({ c }) => c.reading.kind === kind && !likes(c) && !c.not);
  const out: Part[] = [];
  const word = (text: string, tight = false) => out.push(tight ? { text, tight } : { text });
  const term = (i: number, text = chips[i].reading.label) => out.push({ text, chip: i });
  // A kind's terms listed, the last two joined by its "and" or "or" (0.13: that word a tap flips, `flip`).
  const list = (items: { i: number }[], kind?: JoinKind) =>
    items.forEach(({ i }, k) => {
      if (k) {
        if (k < items.length - 1) word(',', true);
        else if (kind) out.push({ text: joinOf(modes, kind), flip: kind });
        else word('or');
      }
      term(i);
    });

  const genres = of('genre');
  const styles = of('style');
  const head = genres.length ? genres : styles;
  if (head.length) list(head, genres.length ? 'genre' : 'style');
  else word('Tracks');
  if (genres.length && styles.length) (word(',', true), word('style'), list(styles, 'style'));
  const tags = of('tag');
  if (tags.length) (word('tagged'), list(tags, 'tag'));
  const by = of('artist');
  if (by.length) (word('by'), list(by));
  const labels = of('label');
  if (labels.length) (word('on'), list(labels));
  const albums = of('album');
  if (albums.length) (word('from the album'), list(albums));

  // The years: one span, from the earliest chip's start to the latest's end; a decade "the 1970s", a span "1970 to 1985".
  const yearAt = ({ c, i }: { c: Chip; i: number }) => ({ i, span: c.reading.value.split('-').map(Number) as [number, number], decade: /^\d{3}0s$/.test(c.reading.label) });
  const years = of('year')
    .map(yearAt)
    .sort((a, b) => a.span[0] - b.span[0] || a.span[1] - b.span[1]);
  /** A span open at one end ("before 1970", "since 2010"): said as itself, no "from". */
  const open = (y: (typeof years)[number]) => y.span[0] <= NO_START || y.span[1] >= NO_END;
  const yearTerm = (y: (typeof years)[number]) => {
    if (y.decade) word('the');
    term(y.i, y.span[0] === y.span[1] || y.decade || open(y) ? chips[y.i].reading.label : `${y.span[0]} to ${y.span[1]}`);
  };
  if (years.length) {
    if (!open(years[0])) word('from');
    const last = years.reduce((a, b) => (b.span[1] > a.span[1] ? b : a));
    yearTerm(years[0]);
    for (const y of years.slice(1)) if (y !== last) (word(',', true), yearTerm(y));
    if (last !== years[0]) (word('to'), yearTerm(last));
  }

  // What it feels and sounds like, one clause: "that feels Calm and sounds like Miles Davis" ("Tracks that feel…").
  const plural = !head.length;
  const moods = of('mood');
  const like = all.filter(({ c }) => likes(c));
  // A Sounds like said in a few words (a built-in crate's: "your week", "Air", "27 tracks"; chipsOf `brief`).
  const sounds = like.length || brief;
  if (moods.length || sounds) word('that');
  if (moods.length) (word(plural ? 'feel' : 'feels'), list(moods, 'mood'));
  if (moods.length && sounds) word('and');
  if (sounds) word(plural ? 'sound like' : 'sounds like');
  if (like.length) list(like, 'like');
  if (brief) (like.length && word('and'), word(brief));

  // What is left out, last: "but not Trap or Drill, not on Def Jam, not by Ann Quartet" (0.13).
  const left = (kind: ChipKind) => all.filter(({ c }) => c.not && c.reading.kind === kind);
  const groups: [string, { i: number }[]][] = [
    ['', left('genre')],
    ['', left('style')],
    ['tagged', left('tag')],
    ['feeling', left('mood')],
    ['by', left('artist')],
    ['on', left('label')],
    ['from the album', left('album')],
  ];
  let first = true;
  const leaving = () => {
    if (first) (out.length && word(',', true), word('but not'));
    else (word(',', true), word('not'));
    first = false;
  };
  for (const [lead, items] of groups) {
    if (!items.length) continue;
    leaving();
    if (lead) word(lead);
    list(items);
  }
  // Years left out, each as itself: "but not from the 2000s or 1975".
  const leftYears = left('year').map(yearAt);
  if (leftYears.length) {
    leaving();
    if (!open(leftYears[0])) word('from');
    leftYears.forEach((y, k) => {
      if (k) (k < leftYears.length - 1 ? word(',', true) : word('or'));
      yearTerm(y);
    });
  }
  return out;
}

/** What `names` says of an id (the server's, with a crate's conditions; decision 0088). */
export type Named = { name: string; kind: 'artist' | 'album' | 'track' };
/** The names a crate's chips hold, by the id (or album key) its conditions use: a term read in the sentence, named for
 *  the conditions' cards. */
export function namesOf(chips: readonly Chip[]): Record<string, Named> {
  const out: Record<string, Named> = {};
  for (const c of chips)
    if (c.reading.kind === 'album' || c.reading.id)
      out[c.reading.id ?? c.reading.value] = { name: c.reading.label, kind: c.reading.kind === 'album' ? 'album' : c.reading.kind === 'track' ? 'track' : 'artist' };
  return out;
}
/** Chips read back from conditions (`chipsOf`) as they were typed where they are the same term: its words and other
 *  readings kept. */
export function keptChips(next: readonly Chip[], had: readonly Chip[]): Chip[] {
  const key = (c: Chip) => `${c.reading.kind}|${c.reading.id ?? c.reading.value}|${c.how ?? ''}|${c.not ? '!' : ''}`;
  const old = new Map(had.map((c) => [key(c), c]));
  return next.map((c) => {
    const o = old.get(key(c));
    return o ? ('own' in c ? { ...o, own: c.own } : o) : c;
  });
}
/** How many seeds a sentence lists; more, or weighted, or fresh (a built-in crate's), are said briefly. */
export const SEEDS_LISTED = 6;
/** A Sounds like said briefly (0.13; the owner: a built-in crate "says itself briefly"): from the crate's own name
 *  ("Sounds like your week" → "your week", "Sounds like Air" → "Air"), else its one seed's name, else how many. */
export function briefOf(of: readonly string[], names: Readonly<Record<string, Named>>, crateName = ''): string {
  const m = /^sounds like (.+)$/i.exec(crateName.trim());
  if (m) return m[1];
  if (of.length === 1 && names[of[0]]) return names[of[0]].name;
  return `${of.length} ${of.length === 1 ? 'track' : 'tracks'}`;
}

/** A crate's conditions read back as its terms (0.13, decision 0088: one crate, two views): the chips, how they join,
 *  the cut, and `rest`, the conditions a sentence cannot say (negated, switched off, an open range, a Sounds like by
 *  how many or weighted, a second row of one kind, any other axis), kept as they are. `names`: the server's for the
 *  ids a condition holds; an id it no longer has reads as gone (removable). The inverse of `conditionsOf`. */
export function chipsOf(
  conditions: readonly Condition[],
  names: Readonly<Record<string, Named>> = {},
  crateName = '',
): { chips: Chip[]; modes: Modes; min: number; rest: Condition[]; brief: { text: string; condition: Condition } | null } {
  const chips: Chip[] = [];
  const modes: Modes = {};
  const rest: Condition[] = [];
  const done = new Set<string>();
  let brief: { text: string; condition: Condition } | null = null;
  let min = CLOSE;
  const kindOfId = (id: string): Named['kind'] => names[id]?.kind ?? (id.startsWith('al-') ? 'album' : id.startsWith('tr-') ? 'track' : 'artist');
  const gone = (k: Named['kind']) => `${k === 'album' ? 'an album' : k === 'track' ? 'a track' : 'an artist'} no longer in the library`;
  for (const c of conditions) {
    const v = c.value;
    const at = `${c.axis}${c.not ? '!' : ''}`;
    const plain = c.enabled !== false && !c.quantifier && !done.has(at);
    const several = v.kind === 'any_of' || v.kind === 'all_of';
    const sayable = isMulti(c.axis) || ((c.axis === 'label' || c.axis === 'artist' || c.axis === 'album') && v.kind === 'any_of');
    // Left out, "but not": any of them only (none of them); "not all of" is the conditions' to say.
    if (plain && several && v.values.length && sayable && (!c.not || v.kind === 'any_of')) {
      const not = c.not ? { not: true } : {};
      for (const value of v.values) {
        if (c.axis === 'artist') chips.push({ term: value, reading: { kind: 'artist', value, label: value }, others: [], how: 'by', own: false, ...not });
        else if (c.axis === 'album') {
          const label = names[value]?.name ?? gone('album');
          chips.push({ term: label, reading: { kind: 'album', value, label }, others: [], ...not });
        } else chips.push({ ...wordChip(c.axis as 'genre' | 'style' | 'mood' | 'tag' | 'label', value), ...not });
      }
      if (!c.not && isMulti(c.axis) && v.values.length > 1) modes[c.axis] = v.kind === 'all_of' ? 'and' : 'or';
    } else if (c.enabled !== false && !c.quantifier && (c.not || !done.has(at)) && c.axis === 'year' && v.kind === 'range' && (v.min !== null || v.max !== null)) {
      // The years' one span, open at one end too ("before 1970", "since 2010"); a span left out each its own row
      // ("but not from the 2000s").
      const [a, z] = [v.min ?? NO_START, v.max ?? NO_END];
      const label =
        v.min === null ? `before ${z + 1}` : v.max === null ? `since ${a}` : a === z ? String(a) : a % 10 === 0 && z === a + 9 ? `${a}s` : `${a}–${z}`;
      chips.push({ term: label, reading: { kind: 'year', value: `${a}-${z}`, label }, others: [], ...(c.not ? { not: true } : {}) });
    } else if (plain && !c.not && v.kind === 'like' && !brief && (v.min === undefined || v.fresh || !!v.weights?.length || v.of.length > SEEDS_LISTED)) {
      // A built-in crate's (by how many, weighted, fresh) or a long list: said briefly, kept as it is.
      brief = { text: briefOf(v.of, names, crateName), condition: c };
      continue;
    } else if (plain && !c.not && v.kind === 'like' && v.min !== undefined && !v.fresh && !v.weights?.length && v.of.length) {
      v.of.forEach((id, k) => {
        const kind = kindOfId(id);
        chips.push({ ...likeChip(kind, id, names[id]?.name ?? gone(kind)), own: k === 0 && !!v.own });
      });
      min = v.min;
      if (v.of.length > 1) modes.like = v.mode === 'either' ? 'or' : 'and';
    } else {
      rest.push(c);
      continue;
    }
    done.add(at);
  }
  return { chips, modes, min, rest, brief };
}

/** A name for it, from its chips: "Jazz · like Bonobo · 1970s". */
export function nameOf(chips: readonly Chip[]): string {
  return chips.map((c) => (likes(c) ? `like ${c.reading.label}` : c.reading.label)).join(' · ');
}

/** The crates made here and not saved that what is being typed may mean (the owner, 2026-10-30: the empty page stays the
 *  regular one until typing starts): none for nothing typed; else those whose name holds every word typed so far, the
 *  last word as a beginning. */
export function recentFor<T extends { chips: readonly Chip[] }>(recent: readonly T[], typed: string): T[] {
  const words = typed.toLowerCase().split(/[\s,;]+/).filter(Boolean);
  if (!words.length) return [];
  return recent.filter((k) => {
    const name = nameOf(k.chips).toLowerCase();
    const parts = name.split(/[\s·]+/).filter(Boolean);
    return words.every((w, i) => (i === words.length - 1 ? parts.some((p) => p.startsWith(w)) : name.includes(w)));
  });
}

/** Where a cut at closeness `min` falls on the closeness curve (how many are at least that alike), and back. */
export const countAt = (curve: readonly number[], min: number) => curve.filter((a) => a >= min).length;
/** The cutoff at `count`: the curve's closeness there, a whole number 0 to 100 as the server's Like `min` takes it
 *  (0.12.1: the curve's own value, 25.37, was refused with 422), rounded down so the track at `count` stays in. */
export const minAt = (curve: readonly number[], count: number) =>
  count > 0 && count <= curve.length ? Math.max(0, Math.min(100, Math.floor(curve[count - 1]))) : 0;

/** A search answer, as far as a crate's terms need it (lib/api ServerSearch). */
type Answer<I extends { subject: string }> = { q: string; groups: Partial<Record<string, { total: number; items: I[] }>>; top: I[] };
/** What a search's terms find, together (0.13; the owner, 2026-10-31: the results under a crate's card are what its
 *  understood terms find, not the whole line): each term's own answer (`answers`, by its words as `words` gives them),
 *  each kind's finds one after another, a find once, the totals added. The terms not asked add nothing. */
export function answerOfTerms<I extends { subject: string }>(q: string, terms: readonly string[], answers: ReadonlyMap<string, Answer<I>>): Answer<I> {
  const groups: Record<string, { total: number; items: I[] }> = {};
  const seen = new Set<string>();
  for (const term of terms) {
    const a = answers.get(words(term).join(' '));
    if (!a) continue;
    for (const [kind, g] of Object.entries(a.groups)) {
      if (!g) continue;
      const into = (groups[kind] ??= { total: 0, items: [] });
      into.total += g.total;
      for (const it of g.items) if (!seen.has(`${kind}\n${it.subject}`)) (seen.add(`${kind}\n${it.subject}`), into.items.push(it));
    }
  }
  return { q, groups, top: [] };
}
