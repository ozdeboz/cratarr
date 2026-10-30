// Queue requests in the queue (0.10; the owner, 2026-10-30): each track asked for shows where it would go, a row as
// any other with a thumbs up and a thumbs down in place of its heart and handle; one accepted is a row of the queue at
// once, until the player's next report has it. No runes, so the tests take it as it is.
import type { QueueRequest } from './api';

type Lane = { lane?: string | null };
export type QueueLine<T> = { kind: 'track'; t: T; i: number } | { kind: 'request'; r: QueueRequest } | { kind: 'accepted'; r: QueueRequest };

/** Where each goes among what comes next: one for next after the tracks played next by hand (the next lane, at the
 *  start), as Play next puts them; one for the end before Autoplay's (which always comes last), as Add to queue does.
 *  Oldest first within each. */
export function withRequests<T extends Lane>(upcoming: readonly T[], asked: readonly QueueRequest[], accepted: readonly QueueRequest[] = []): QueueLine<T>[] {
  const tracks: QueueLine<T>[] = upcoming.map((t, i) => ({ kind: 'track', t, i }));
  let nextAt = 0;
  while (nextAt < upcoming.length && upcoming[nextAt].lane === 'next') nextAt++;
  let lastAt = upcoming.length;
  while (lastAt > nextAt && upcoming[lastAt - 1].lane === 'autoplay') lastAt--;
  const lines = (how: 'next' | 'last') =>
    [...accepted.map((r) => ({ kind: 'accepted' as const, r })), ...asked.map((r) => ({ kind: 'request' as const, r }))]
      .filter((x) => x.r.how === how)
      .sort((a, b) => a.r.at - b.r.at);
  return [...tracks.slice(0, nextAt), ...lines('next'), ...tracks.slice(nextAt, lastAt), ...lines('last'), ...tracks.slice(lastAt)];
}

/** Whether the requests show: as this device's user last chose, else on while they let others follow them. */
export const requestsShown = (chosen: boolean | null, followable: boolean) => chosen ?? followable;

/** How long an accepted one shows as a row of its own at most, the player's report not having it yet (ms). */
export const ACCEPTED_MS = 10_000;
/** An accepted one still to show as its own row: the queue holds its track no more often than when it was accepted
 *  (`before`), and not for long. */
export const stillComing = (a: { r: QueueRequest; before: number; at: number }, upcoming: readonly { id: string }[], now: number) =>
  now - a.at < ACCEPTED_MS && upcoming.filter((t) => t.id === a.r.track.id).length <= a.before;
