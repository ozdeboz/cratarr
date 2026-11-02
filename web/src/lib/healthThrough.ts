// Health laid out as Settings (0.13; the owner, 2026-10-31, mockup C): the page's problems grouped by their domain, and
// Work through: a problem's tracks looked for one at a time in one sheet that moves on by itself after Add or Skip
// (decision 0080: one track at a time, each its own choice, never in bulk). Pure, so the tests take them as they are.
import type { HealthItem } from './api';

/** The problems in the report's order (worst first), grouped by domain: a domain where its worst problem stands. */
export function byDomain<T extends { domain: string }>(list: readonly T[]): { domain: string; problems: T[] }[] {
  const out: { domain: string; problems: T[] }[] = [];
  for (const p of list) {
    const g = out.find((x) => x.domain === p.domain);
    if (g) g.problems.push(p);
    else out.push({ domain: p.domain, problems: [p] });
  }
  return out;
}

/** The problems by Health's area, in the areas' order, each area's most urgent first (red, then amber, then grey;
 *  the report's order within each: the owner, 2026-11-01). */
export function byArea<T extends { area: string; urgent: boolean; finding: { level: 'critical' | 'warning' | 'info' } | null }>(
  list: readonly T[],
  areas: readonly string[],
): { area: string; problems: T[] }[] {
  const rank = (p: T) => ({ bad: 0, warn: 1, off: 2 })[dotOf(p)];
  return areas
    .map((area) => ({ area, problems: list.filter((p) => p.area === area).sort((a, b) => rank(a) - rank(b)) }))
    .filter((g) => g.problems.length);
}

/** A problem's dot (lib/settings/Item's): red where it needs you, amber a warning, grey to know. */
export function dotOf(p: { urgent: boolean; finding: { level: 'critical' | 'warning' | 'info' } | null }): 'bad' | 'warn' | 'off' {
  if (!p.finding) return p.urgent ? 'bad' : 'warn';
  return p.finding.level === 'critical' ? 'bad' : p.finding.level === 'warning' ? 'warn' : 'off';
}

/** One stop of a Work through: an album, its missing tracks one after another (`missing`: how many the report names);
 *  or one track of it to replace (`replace`: its song id, a damaged file). */
export type Stop = { source: string; album: string; title: string; missing: number; replace: string | null };

/** What a problem's items can be worked through as: their missing tracks, their damaged files, or nothing. */
export function throughKind(items: readonly HealthItem[]): 'missing' | 'damaged' | null {
  if (items.some((x) => x.album?.[0] === 'cratarr' && x.missing?.length)) return 'missing';
  if (items.some((x) => x.album && x.track)) return 'damaged';
  return null;
}

const byTitle = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true });

/** The albums missing tracks, the most missing first, then by title: each a stop. */
export function missingStops(items: readonly HealthItem[]): Stop[] {
  return items
    .filter((x) => x.album?.[0] === 'cratarr' && x.missing?.length)
    .map((x) => ({ source: x.album![0], album: x.album![1], title: x.title, missing: x.missing!.length, replace: null }))
    .sort((a, b) => b.missing - a.missing || byTitle(a.title, b.title) || a.album.localeCompare(b.album));
}

/** The damaged files, album by album (the album with the most first), each a stop; within an album as the report
 *  lists them. */
export function damagedStops(items: readonly HealthItem[]): Stop[] {
  const tracks = items.filter((x) => x.album && x.track);
  const per = new Map<string, number>();
  for (const x of tracks) per.set(x.album![1], (per.get(x.album![1]) ?? 0) + 1);
  const first = new Map<string, number>();
  tracks.forEach((x, i) => first.has(x.album![1]) || first.set(x.album![1], i));
  return tracks
    .map((x, i) => ({ stop: { source: x.album![0], album: x.album![1], title: x.title, missing: 1, replace: x.track! }, i }))
    .sort((a, b) => per.get(b.stop.album)! - per.get(a.stop.album)! || first.get(a.stop.album)! - first.get(b.stop.album)! || a.i - b.i)
    .map((x) => x.stop);
}

/** How many tracks a Work through holds. */
export const throughTotal = (stops: readonly Stop[]) => stops.reduce((n, s) => n + s.missing, 0);

/** A track of a release, as lib/replaceTrack reads it. */
export type ThroughTrack = { disc: number; position: number; recording: string; have: { source: string; id: string } | null };

/** The track a stop wants next, of its release's tracks: the damaged file's own, or the first missing one in track
 *  order; none dealt with already (`handled`: added or skipped); null: the stop is done. */
export function nextWanted<T extends ThroughTrack>(tracks: readonly T[], stop: Stop, handled: readonly string[]): T | null {
  const open = (t: T) => !handled.includes(t.recording);
  if (stop.replace) return tracks.find((t) => t.have?.id === stop.replace && open(t)) ?? null;
  return [...tracks].sort((a, b) => a.disc - b.disc || a.position - b.position).find((t) => !t.have && open(t)) ?? null;
}

/** Where a Work through stands: the stop it is at, the tracks of it dealt with, how many it has dealt with in all. */
export type Through = { at: number; handled: string[]; count: number };
export const startThrough = (): Through => ({ at: 0, handled: [], count: 0 });
/** One track dealt with (added or skipped). */
export const handledOne = (t: Through, recording: string): Through => ({ at: t.at, handled: [...t.handled, recording], count: t.count + 1 });
/** On to the next stop (the one it was at done, or skipped whole where its release cannot be read). */
export const nextStop = (t: Through): Through => ({ at: t.at + 1, handled: [], count: t.count });
/** On past a stop not dealt with (its release could not be read): its tracks counted as passed. */
export const skipStop = (t: Through, stop: Stop): Through => ({ at: t.at + 1, handled: [], count: t.count + Math.max(0, stop.missing - t.handled.length) });
/** "2 of 1,032": the track it is at, of all. */
export const throughLine = (t: Through, total: number) => `${Math.min(t.count + 1, Math.max(total, 1)).toLocaleString()} of ${total.toLocaleString()}`;
