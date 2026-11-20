// A record not had (C54, W9): its page from the catalogue, of an album the library has no copy of. Its rows are its list's,
// none of them playable; no heart, no Resume, nothing of a copy's. Want is in its header, or Requested while its request is
// open; its Bandcamp preview plays from there where it has one (OQ14). One the library has through another record (`have`)
// says so, and opens that one, with no Want. A retired one says until when it was in Music (OQ16). One with no rows says
// why (F1), with the standard loader in its track list's place while the server reads its list (F6), and is asked again
// meanwhile. A record's About names where it comes from (F7); an artist's and a label's sections read in the background say
// so and why they have none in the same words (F8, F10). Pure.
import type { ListRead, NoList, RecordSource } from './api';
import type { EntryId } from './menuTable';
import { requestOpen, standing, type Asked } from './requestState';
import { when } from './settings/when';

/** What the page reads of a record not had's view. */
export type NotHad = {
  not_had?: boolean;
  have?: string | null;
  bandcamp?: string | null;
  was_until?: number | null;
  request?: Asked | null;
};

/** Its header's actions, in the bar's order: the preview's Play, Shuffle, Play next and Add to queue, then Want or Requested. */
export type NotHadAction = 'play' | 'shuffle' | 'next' | 'last' | 'want' | 'requested';
/** A line under its title: the record the library has it as, where its open request stands (W12: the server's state), or until
 *  when it was in Music. */
export type NotHadLine = { kind: 'have'; record: string; text: string } | { kind: 'request'; text: string } | { kind: 'was'; text: string };
export type NotHadPage = { heart: boolean; resume: boolean; actions: NotHadAction[]; lines: NotHadLine[] };

/** "Was in Music until 3 March 2026" (OQ16), in the browser's language. */
export const wasInMusic = (t: number) => `Was in Music until ${new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}`;

/** What its page shows: nothing of a copy's (no heart, no Resume); the preview's actions where it has a Bandcamp page; Want, or
 *  Requested while its request is open, but none where the library has it through another record. */
export function notHadPage(v: NotHad, said: (t: number) => string = when): NotHadPage {
  const play: NotHadAction[] = v.bandcamp ? ['play', 'shuffle', 'next', 'last'] : [];
  const lines: NotHadLine[] = [];
  const open = requestOpen(v.request);
  if (v.have) lines.push({ kind: 'have', record: v.have, text: 'In your library' });
  if (open && v.request) lines.push({ kind: 'request', text: standing(v.request, said) });
  if (v.was_until) lines.push({ kind: 'was', text: wasInMusic(v.was_until) });
  const ask: NotHadAction[] = v.have ? [] : [open ? 'requested' : 'want'];
  return { heart: false, resume: false, actions: [...play, ...ask], lines };
}

/** Its menu: only what needs no copy of it (the album menu's own entries; Want is in its header). */
export const NOT_HAD_ENTRIES: readonly EntryId[] = ['view', 'goArtist', 'share', 'webSearch', 'geniusSearch'];
export const offeredNotHad = (e: EntryId) => NOT_HAD_ENTRIES.includes(e);

/** The sources lists are read from, by name: a record's (MusicBrainz, Discogs, Bandcamp: the server's counting order), an
 *  artist's (ListenBrainz, Last.fm) too; another by its id as the server says it. */
const SOURCES: Record<string, string> = { musicbrainz: 'MusicBrainz', discogs: 'Discogs', bandcamp: 'Bandcamp', listenbrainz: 'ListenBrainz', lastfm: 'Last.fm' };
const COUNTING_ORDER: readonly NoList['source'][] = ['musicbrainz', 'discogs', 'bandcamp'];
const sourceName = (id: string) => SOURCES[id] ?? id;
/** "A", "A and B", "A, B and C". */
const names = (xs: string[]) => (xs.length < 2 ? xs.join('') : `${xs.slice(0, -1).join(', ')} and ${xs[xs.length - 1]}`);

/** Why a list is not there (F1's words, one way for a record's tracklist and a section's list): its sources switched off,
 *  held back until when, unanswered, then with none, each part's sources in the order given. */
function why(list: readonly { source: string; said: string; until?: number }[], said: (t: number) => string): string {
  const saying = (s: string) => list.filter((x) => x.said === s);
  const part = (s: string, one: string, more: string) => {
    const xs = saying(s).map((x) => sourceName(x.source));
    return xs.length ? [`${names(xs)} ${xs.length > 1 ? more : one}`] : [];
  };
  return [
    ...part('off', 'is switched off', 'are switched off'),
    ...saying('held').map((x) => `${sourceName(x.source)} is held back${x.until ? ` until ${said(x.until)}` : ''}`),
    ...part('unanswered', "didn't answer", "didn't answer"),
    ...part('none', 'has none', 'have none'),
  ].join('; ');
}

/** An empty album's line (F1): a record not had says why it has no tracks once the server's read of its list has ended, from
 *  its view's `no_list`, each part's sources in counting order. Anything else with no rows: "No tracks yet." as ever. */
export function emptyLine(v: { not_had?: boolean; looking?: boolean; no_list?: NoList[] }, said: (t: number) => string = when): string {
  if (!v.not_had) return 'No tracks yet.';
  const list = [...(v.no_list ?? [])].sort((a, b) => COUNTING_ORDER.indexOf(a.source) - COUNTING_ORDER.indexOf(b.source));
  if (!list.length) return 'No tracklist: it has no reference to read one from';
  return `No tracklist: ${why(list, said)}`;
}

/** What a one-list view's track list shows where it has no rows (F6): for a record not had whose list the server reads, the
 *  standard loader saying the source it reads now, never a line; else the empty line (F1). */
export type EmptyPlace = { reading: string } | { line: string };
export function emptyPlace(v: { not_had?: boolean; looking?: boolean; reading?: string; no_list?: NoList[] }, said: (t: number) => string = when): EmptyPlace {
  if (v.not_had && v.looking) return { reading: v.reading ? `Reading its tracklist from ${sourceName(v.reading)}` : 'Reading its tracklist' };
  return { line: emptyLine(v, said) };
}

/** The rows a record not had shows from an answer (F6): once it showed rows, an answer with none while the server reads again
 *  keeps them, so the loader never comes back for it. */
export const keptRows = <T>(answer: T[], shown: T[] | null | undefined, v: { not_had?: boolean; looking?: boolean }): T[] =>
  v.not_had && v.looking && !answer.length && shown?.length ? shown : answer;

/** What a section filled by a background read shows (an artist's Not in your library and recommendations, F8; a label's Not
 *  in your library, F10), from its answer and how many rows it has to show: its rows; while it is read, the section's loader
 *  naming the source it reads now; once read, why it has none in F1's words; nothing where the read ended with none and no
 *  source failed. Never absent while it is read or after it failed. */
export type SectionPlace = 'rows' | { reading: string } | { line: string } | null;
export function sectionPlace(v: ListRead, rows: number, said: (t: number) => string = when): SectionPlace {
  if (rows) return 'rows';
  if (v.looking) return { reading: v.reading ? `Reading ${sourceName(v.reading)}…` : 'Reading…' };
  return v.no_list?.length ? { line: `No list: ${why(v.no_list, said)}` } : null;
}

/** The mark of the source a record's tracklist is read from (SB18): About's chip and the References sheet say it alike. */
export const TRACKLIST = 'Tracklist';

/** A source's chip in About's Sources row (F19). */
export type SourceChip = { name: string; url: string; edition?: string; tracks: boolean };
const ALBUM_KINDS = ['release-group', 'master', 'album'];

/** A record's About's Sources row (F7, F19): one chip per source, in counting order, by its source's name, opening the album's
 *  page there (its release group, master or album page; a release where that is its only reference), the exact edition the
 *  record holds kept on the same chip (`edition`, never a second chip); `tracks` where its tracklist is read from there. Discogs'
 *  page from About's lookup (`discogs`) is the Discogs chip where the record holds no Discogs reference: Elsewhere folds into
 *  Sources (F19.4). https pages only. None: no row. */
export function sourceChips(sources: readonly RecordSource[] | null | undefined, discogs?: string | null): SourceChip[] {
  const https = (u: string | null | undefined): u is string => !!u && /^https:\/\//.test(u);
  const by = new Map<string, RecordSource[]>();
  for (const s of sources ?? []) if (https(s.url)) by.set(s.source, [...(by.get(s.source) ?? []), s]);
  if (https(discogs) && !by.has('discogs')) by.set('discogs', [{ source: 'discogs', kind: 'master', url: discogs }]);
  const order = (s: string) => {
    const i = COUNTING_ORDER.indexOf(s as NoList['source']);
    return i < 0 ? COUNTING_ORDER.length : i;
  };
  return [...by]
    .sort(([a], [b]) => order(a) - order(b))
    .map(([source, refs]) => {
      const page = refs.find((r) => ALBUM_KINDS.includes(r.kind)) ?? refs[0];
      const edition = refs.find((r) => r !== page && r.kind === 'release' && r.url !== page.url);
      return { name: sourceName(source), url: page.url, ...(edition ? { edition: edition.url } : {}), tracks: refs.some((r) => !!r.tracks) };
    });
}

/** "1 track", "11 tracks". */
const tracksText = (n: number) => `${n.toLocaleString()} ${n === 1 ? 'track' : 'tracks'}`;

/** A list head's count of tracks (F17): a record not had counts the rows it shows (its page's words narrowing them too), none where
 *  it shows none, never "0 tracks" beside a list or a loader; anything else its playable rows, as before. */
export const listCount = (v: { notHad: boolean; shown: number; playable: number }): string => (v.notHad ? (v.shown ? tracksText(v.shown) : '') : tracksText(v.playable));

/** How long a record not had waits to be asked again while the server reads its list (n: asks made for this opening), and
 *  an artist's or a label's section read in the background (F8, F10): soon at first, then seldom, with no last ask (the read
 *  is cut at the server's own limit, F1.9). */
export const lookAgainAfter = (n: number): number => (n < 8 ? 4000 : 30000);

/** A row's second line on an album's page: its own artist where it is another than the album's; a file gone, said so, but
 *  never on a record not had, whose rows have no files to lose. */
export function albumRowSub(r: { missing?: boolean; artist?: string | null }, notHad: boolean, albumArtist: string | null | undefined): string | undefined {
  if (r.missing && !notHad) return 'Missing file';
  return r.artist && r.artist !== albumArtist ? r.artist : undefined;
}
