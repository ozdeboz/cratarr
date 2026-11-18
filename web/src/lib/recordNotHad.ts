// A record not had (C54, W9): its page from the catalogue, of an album the library has no copy of. Its rows are its list's,
// none of them playable; no heart, no Resume, nothing of a copy's. Want is in its header, or Requested while its request is
// open; its Bandcamp preview plays from there where it has one (OQ14). One the library has through another record (`have`)
// says so, and opens that one, with no Want. A retired one says until when it was in Music (OQ16). Pure.
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

/** A row's second line on an album's page: its own artist where it is another than the album's; a file gone, said so, but
 *  never on a record not had, whose rows have no files to lose. */
export function albumRowSub(r: { missing?: boolean; artist?: string | null }, notHad: boolean, albumArtist: string | null | undefined): string | undefined {
  if (r.missing && !notHad) return 'Missing file';
  return r.artist && r.artist !== albumArtist ? r.artist : undefined;
}
