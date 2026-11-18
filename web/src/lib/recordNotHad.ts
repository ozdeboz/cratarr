// A record not had (C54, W9): its page from the catalogue, of an album the library has no copy of. Its rows are its list's,
// none of them playable; no heart, no Resume, nothing of a copy's. Want is in its header, or Requested while its request is
// open; its Bandcamp preview plays from there where it has one (OQ14). One the library has through another record (`have`)
// says so, and opens that one, with no Want. A retired one says until when it was in Music (OQ16). One with no rows says
// why (F1), and is asked again while the server reads its list. Pure.
import type { NoList } from './api';
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

/** The sources a record's list is read from, in the server's counting order (MusicBrainz, Discogs, Bandcamp), by name. */
const SOURCES: Record<NoList['source'], string> = { musicbrainz: 'MusicBrainz', discogs: 'Discogs', bandcamp: 'Bandcamp' };
const COUNTING_ORDER = Object.keys(SOURCES) as NoList['source'][];
/** "A", "A and B", "A, B and C". */
const names = (xs: string[]) => (xs.length < 2 ? xs.join('') : `${xs.slice(0, -1).join(', ')} and ${xs[xs.length - 1]}`);

/** An empty album's line (F1): a record not had says why it has no tracks, from its view's `no_list`: its sources switched
 *  off, held back until when, unanswered, then with none, each part's sources in counting order; "Reading its tracklist…"
 *  while the server reads it. Anything else with no rows: "No tracks yet." as ever. */
export function emptyLine(v: { not_had?: boolean; looking?: boolean; no_list?: NoList[] }, said: (t: number) => string = when): string {
  if (!v.not_had) return 'No tracks yet.';
  if (v.looking) return 'Reading its tracklist…';
  const list = [...(v.no_list ?? [])].sort((a, b) => COUNTING_ORDER.indexOf(a.source) - COUNTING_ORDER.indexOf(b.source));
  if (!list.length) return 'No tracklist: it has no reference to read one from';
  const saying = (s: NoList['said']) => list.filter((x) => x.said === s);
  const part = (s: NoList['said'], one: string, more: string) => {
    const xs = saying(s).map((x) => SOURCES[x.source]);
    return xs.length ? [`${names(xs)} ${xs.length > 1 ? more : one}`] : [];
  };
  const parts = [
    ...part('off', 'is switched off', 'are switched off'),
    ...saying('held').map((x) => `${SOURCES[x.source]} is held back${x.until ? ` until ${said(x.until)}` : ''}`),
    ...part('unanswered', "didn't answer", "didn't answer"),
    ...part('none', 'has none', 'have none'),
  ];
  return `No tracklist: ${parts.join('; ')}`;
}

/** How long a record not had waits to be asked again while the server reads its list (n: asks made for this opening):
 *  soon at first, then seldom, with no last ask (the read is cut at the server's own limit, F1.9). */
export const lookAgainAfter = (n: number): number => (n < 8 ? 4000 : 30000);

/** A row's second line on an album's page: its own artist where it is another than the album's; a file gone, said so, but
 *  never on a record not had, whose rows have no files to lose. */
export function albumRowSub(r: { missing?: boolean; artist?: string | null }, notHad: boolean, albumArtist: string | null | undefined): string | undefined {
  if (r.missing && !notHad) return 'Missing file';
  return r.artist && r.artist !== albumArtist ? r.artist : undefined;
}
