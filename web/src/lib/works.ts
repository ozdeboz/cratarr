// Classical's works and movements (0.12, decision 0086; the owner, 2026-10-30): a composer's works in the order they are
// shown, a recording's movements in their work's order, and an album's tracks grouped under the work their movements
// belong to. Pure: the pages draw what this says.

/** A work as a composer's page lists it (GET /api/composers/{key}/works). */
export type WorkListed = { work: string; title: string; key?: string | null; catalogue?: string | null; parts: number; recordings: number };
/** A work named on an album's track: its own work (a movement, say) and the work that holds it, where known. */
export type TrackWork = { mbid: string; title: string; parent?: { mbid: string; title: string } | null; part_number?: number | null };

/** A composer's works as their page shows them (the owner: "works first"): the most recorded first, then by title. */
export function worksInOrder<T extends Pick<WorkListed, 'title' | 'recordings'>>(list: readonly T[]): T[] {
  return [...list].sort((a, b) => b.recordings - a.recordings || a.title.localeCompare(b.title, undefined, { numeric: true, sensitivity: 'base' }));
}

/** A work's second line: how many of its recordings the library holds, its catalogue number and key where known. */
export function workLine(w: Pick<WorkListed, 'recordings' | 'catalogue' | 'key'>): string {
  const n = `${w.recordings} recording${w.recordings === 1 ? '' : 's'}`;
  return [n, w.catalogue, w.key].filter(Boolean).join(' · ');
}

/** A recording's tracks in its work's order: by their part's place among the work's parts, a track whose part is not
 *  known keeping its place among those (as the album has them). */
export function inPartOrder<T extends { part?: string | null }>(tracks: readonly T[], parts: readonly { mbid: string; number?: number | null }[]): T[] {
  const ranked = [...parts].map((p, i) => ({ p, i })).sort((a, b) => (a.p.number ?? Infinity) - (b.p.number ?? Infinity) || a.i - b.i);
  const at = new Map(ranked.map((x, n) => [x.p.mbid, n]));
  return tracks
    .map((t, i) => ({ t, i, at: t.part ? (at.get(t.part) ?? Infinity) : Infinity }))
    .sort((a, b) => a.at - b.at || a.i - b.i)
    .map((x) => x.t);
}

const loose = (s: string) =>
  s
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
/** How well a title's head names the work: 3 the same words; 2 a short form of it (whole words from its start, its
 *  number kept: "Symphony No. 5" for "Symphony No. 5 in C minor, Op. 67", never "Symphony No"); 1 a longer form (the
 *  work's words and more); 0 not it. */
function naming(head: string, work: string): number {
  const [a, b] = [loose(head), loose(work)];
  if (!a || !b) return 0;
  if (a === b) return 3;
  if (a.length >= 6 && b.startsWith(a) && b[a.length] === ' ') {
    const number = b.split(' ').find((w) => /\d/.test(w));
    return !number || a.split(' ').includes(number) ? 2 : 0;
  }
  return b.length >= 6 && a.startsWith(b) && a[b.length] === ' ' ? 1 : 0;
}
const SEPARATORS = [': ', ' - ', ' – ', ' — ', '. '];

/** What follows the work's name in a title ("Symphony No. 5 in C minor, Op. 67: I. Allegro con brio" → "I. Allegro con
 *  brio"), as the title has it (its numbering kept: roman, arabic or none); null where the title does not start with
 *  the work's name. Of the places the name could end, the best: the same words, else the longest short form, else the
 *  shortest longer one. */
function after(title: string, work: string): string | null {
  let best: { rank: number; head: number; rest: string } | null = null;
  for (let at = 0; at < title.length; at++) {
    const sep = SEPARATORS.find((s) => title.startsWith(s, at));
    if (!sep) continue;
    const rest = title.slice(at + sep.length).trim();
    const rank = rest ? naming(title.slice(0, at), work) : 0;
    if (!rank) continue;
    const better = !best || rank > best.rank || (rank === best.rank && (rank === 2 ? at > best.head : at < best.head));
    if (better) best = { rank, head: at, rest };
  }
  return best?.rest ?? null;
}

/** A movement's name under its work's heading (the owner: "named by movement only"): the track's title without the
 *  work's name, else its part's title without it, else the track's title as it is. */
export function movementLabel(track: string, work: string, part?: string | null): string {
  return after(track, work) ?? (part ? after(part, work) : null) ?? track;
}

/** One line of an album as grouped: a work's heading (its movements' places in the list), or a track; a track under a
 *  heading is a movement, shown by its movement's name. */
export type WorkLine<R> =
  | { kind: 'work'; work: { mbid: string; title: string }; rows: R[]; at: number[] }
  | { kind: 'track'; row: R; at: number; movement: string | null };

/** What a work's menu plays when no movements are given (0.12: a work found in search): its first recording's tracks
 *  (the oldest, as the server lists them) in the work's order; none without a recording. */
export function firstRecording(w: { parts: readonly { mbid: string; number?: number | null }[]; recordings: readonly { tracks: readonly { id: string; part?: string | null }[] }[] }): string[] {
  const first = w.recordings[0];
  return first ? inPartOrder(first.tracks, w.parts).map((t) => t.id) : [];
}

/** An album's rows grouped by work (the owner: "a work heading over its movements"): two or more tracks in a row whose
 *  works are parts of the same work go under that work's heading. A track whose work has no parent yet (MusicBrainz's
 *  parts come in over days), one alone, or none stays as it is, so a heading is never half there. */
export function groupByWork<R extends { name?: string | null; file?: string; work?: TrackWork | null }>(rows: readonly R[]): WorkLine<R>[] {
  const parentOf = (r: R) => r.work?.parent ?? null;
  const out: WorkLine<R>[] = [];
  for (let i = 0; i < rows.length; ) {
    const p = parentOf(rows[i]);
    let j = i + 1;
    if (p) while (j < rows.length && parentOf(rows[j])?.mbid === p.mbid) j++;
    if (p && j - i >= 2) {
      const at = Array.from({ length: j - i }, (_, k) => i + k);
      out.push({ kind: 'work', work: { mbid: p.mbid, title: p.title }, rows: at.map((k) => rows[k]), at });
      for (const k of at) {
        const r = rows[k];
        out.push({ kind: 'track', row: r, at: k, movement: movementLabel(r.name ?? r.file ?? '', p.title, r.work?.title) });
      }
    } else {
      for (let k = i; k < j; k++) out.push({ kind: 'track', row: rows[k], at: k, movement: null });
    }
    i = j;
  }
  return out;
}
