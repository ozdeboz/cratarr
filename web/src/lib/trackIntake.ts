// Health's albums missing tracks, many at once (0.13, mockup D on C; the owner, 2026-10-31: "Bulk search, you approve",
// decision 0080 amended): the review list's rules, pure. A track's row says its chosen copy or where it stands; only a
// found one can be ticked, and none is ticked until the owner ticks it; Add sends the ticked ones, each with the copy
// picked for it (the chosen one unless another was).
import { copyLine, type Candidate, type Intake, type IntakeItem, type IntakeState } from './replaceTrack';

/** States still under way: the list is asked again while any is. */
const UNDER_WAY: ReadonlySet<IntakeState> = new Set(['queued', 'searching', 'approved', 'fetching', 'checking']);
/** States that are over: Clear finished takes them. */
const OVER: ReadonlySet<IntakeState> = new Set(['none', 'failed', 'added', 'refused']);

/** The copy that goes in for an item: the one picked for it while it is among its candidates, else the chosen one. */
export function copyOf(item: IntakeItem, picked: string | undefined): Candidate | null {
  const of = (id: string | null | undefined) => (id ? (item.candidates.find((c) => c.id === id) ?? null) : null);
  return of(picked) ?? of(item.chosen);
}

/** Whether it can be ticked: found, with a copy to take. */
export const tickable = (item: IntakeItem) => item.state === 'found' && item.candidates.length > 0;

/** The ids that can be ticked (Tick all found). */
export const foundIds = (items: readonly IntakeItem[]) => items.filter(tickable).map((i) => i.id);

/** Its row's line: the copy that would go in where found, else where it stands. */
export function itemLine(item: IntakeItem, picked?: string): string {
  switch (item.state) {
    case 'queued':
      return 'Waiting to be looked for';
    case 'searching':
      return 'Looking for it';
    case 'found': {
      const c = copyOf(item, picked);
      return c ? copyLine(c) : 'Not found';
    }
    case 'none':
      return 'Not found';
    case 'failed':
      return item.why || 'The search did not finish';
    case 'approved':
      return 'To be added';
    case 'fetching':
      return 'Downloading';
    case 'checking':
      return 'Checking it';
    case 'added':
      return 'Added';
    case 'refused':
      return item.why || 'It did not pass the checks';
  }
}

/** Where it went wrong, said at its row's end; the reason (the single-track gate's own sentence, some long) is its
 *  line, never squeezed beside it (a5, 2026-10-31). */
export const itemEnd = (item: IntakeItem): string | null =>
  item.state === 'refused' ? 'Refused' : item.state === 'failed' ? 'Failed' : null;

/** Its line's tone: what went wrong said so. */
export const itemTone = (item: IntakeItem): 'bad' | 'warn' | null =>
  item.state === 'refused' || item.state === 'failed' ? 'bad' : item.state === 'none' ? 'warn' : null;

/** What Add sends: the ticked items that can still be taken, each with its copy (picked, else chosen). */
export function approvePayload(items: readonly IntakeItem[], ticked: ReadonlySet<string>, picks: Readonly<Record<string, string>>): { id: string; candidate?: string }[] {
  return items
    .filter((i) => ticked.has(i.id) && tickable(i))
    .flatMap((i) => {
      const c = copyOf(i, picks[i.id]);
      return c ? [{ id: i.id, candidate: c.id }] : [];
    });
}

/** Whether the list is to be asked again (every couple of seconds): the search runs, or a track is on its way in. */
export const underWay = (intake: Intake | null) => !!intake && (intake.running || intake.items.some((i) => UNDER_WAY.has(i.state)));

/** Whether any item is over (Clear finished has something to take). */
export const anyOver = (items: readonly IntakeItem[]) => items.some((i) => OVER.has(i.state));

/** How far the looking has come: those looked for (past queued and searching) of all. */
export function lookedFor(items: readonly IntakeItem[]): { done: number; total: number } {
  const done = items.filter((i) => i.state !== 'queued' && i.state !== 'searching').length;
  return { done, total: items.length };
}

/** Ticks kept only where they can still be (a track gone or no longer found loses its tick). */
export function keptTicks(ticked: ReadonlySet<string>, items: readonly IntakeItem[]): Set<string> {
  const can = new Set(foundIds(items));
  return new Set([...ticked].filter((id) => can.has(id)));
}
