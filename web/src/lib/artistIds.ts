// One artist page by id (W10, C55): Cratarr's own artist is asked for by their id for life (`ar-<n>`), so two artists of one
// name are two pages. Go to artist opens by name where only a name is known; the server says who that is, and the panel moves to
// their id. Bandcamp's previews and Discover's suggestions go to Cratarr's page; another server's artist (server mode) and the
// device's own music (local mode) stay by name, as they have no Cratarr ids. Pure.

/** Cratarr's own artist's id for life, as the server serves it. */
export const isArtistId = (s: string) => /^ar-\d{1,15}$/.test(s);

/** How an artist page asks the server: by id where its subject is one, else by name. */
export const artistAsk = (nameOrId: string): { id: string } | { name: string } => (isArtistId(nameOrId) ? { id: nameOrId } : { name: nameOrId });

/** Whose page Go to artist opens: Bandcamp's preview and a Discover suggestion are Cratarr's; any other source stays its own. */
export const artistSource = (source: string) => (source === 'bandcamp' || source === 'discover' ? 'cratarr' : source);

/** Where an artist page opened by name moves once the server says who it is: to their id, on Cratarr's own page only; none where it
 *  was opened by id already, or the server knows nobody of the name. */
export function movedTo(source: string, asked: string, answered: { id?: string | null }): string | null {
  if (source !== 'cratarr' || isArtistId(asked) || !answered.id || !isArtistId(answered.id)) return null;
  return answered.id;
}

/** The page's title: the name the server answers, else the one asked (an id, not a name, says nothing until then). Plain, even
 *  where namesakes share it (the owner, 2026-11-06: a number only where a view shows two). */
export const artistTitle = (asked: string, answered: string | null | undefined) =>
  answered && !isArtistId(answered) ? answered : isArtistId(asked) ? '' : asked;

/** "Rone (2)": an artist's number, only where the list shown holds another of the same name, and the server gave them one. */
export function artistLabel(name: string, number: number | null | undefined, shown: readonly string[]): string {
  const same = (x: string) => x.trim().toLowerCase() === name.trim().toLowerCase();
  return number && shown.filter(same).length > 1 ? `${name} (${number})` : name;
}

/** How a related or Beyond artist opens (W10): by their id where Cratarr has them, else by their MusicBrainz id (the server opens
 *  them, `POST /api/artists/open`), else by name. Never a Bandcamp album or MusicBrainz's page. */
export type ArtistOpen = { id: string } | { mbid: string; name: string } | { name: string };
export function artistOpen(a: { id?: string | null; mbid?: string | null; name: string }): ArtistOpen {
  if (a.id && isArtistId(a.id)) return { id: a.id };
  if (a.mbid) return { mbid: a.mbid, name: a.name };
  return { name: a.name };
}
