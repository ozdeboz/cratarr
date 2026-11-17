// Beyond in the search (decision 0076, C57; W12): MusicBrainz's and Discogs' albums for the words, each by the record that holds
// its reference, else by what its source knows it by. A row is left out where the library has it, or where what Cratarr knows
// elsewhere lists it already, by its record or its release group; never by its name: two albums of one name are two (C57). Pure.

/** A Beyond album as the page holds it (lib/api `BeyondAlbum`). */
type Beyond = { record?: string | null; release_group?: string | null; discogs?: number | null; have?: boolean };

/** A row's key and subject: its record, else its release group, else its Discogs master. */
export const beyondKey = (a: Beyond) => a.record ?? a.release_group ?? `d${a.discogs ?? ''}`;
export const beyondSubject = (a: Beyond) => `beyond:${beyondKey(a)}`;

/** What the other groups' results are known by: each subject whole (`album:cratarr/al-<n>`) and its id (a release group). */
export const knownOf = (subjects: readonly string[]) => new Set(subjects.flatMap((s) => [s, s.slice(s.indexOf(':') + 1)]));

/** The Beyond rows to show, each once: none the library has, none listed already by its record or group. */
export function beyondFound<A extends Beyond>(albums: readonly A[], known: ReadonlySet<string>): A[] {
  const seen = new Set<string>();
  return albums.filter((a) => {
    const key = beyondKey(a);
    if (a.have || seen.has(key)) return false;
    seen.add(key);
    if (a.record && known.has(`album:cratarr/${a.record}`)) return false;
    return !(a.release_group && known.has(a.release_group));
  });
}
