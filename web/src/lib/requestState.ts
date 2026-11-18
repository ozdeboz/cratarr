// A request's state as the server works it out (W5: one state every view reads, `requests::RequestState`), never worked out
// again here from its status, its album or what Lidarr said. Pure.
import { GB, roomLine } from './room';

/** What Lidarr says of a request it was handed: looking, fetching (how far, 0 to 1), or stuck (Soulseek's turn). */
export type LidarrWord = { state: 'looking' } | { state: 'fetching'; done: number } | { state: 'stuck' };
/** What Soulseek says of a request's record: searched now, found and waiting for the tick, fetching (0 to 100), or waiting
 *  and stuck, each with why where it says. */
export type SoulseekWord =
  | { state: 'searching' }
  | { state: 'found' }
  | { state: 'fetching'; percent: number }
  | { state: 'waiting'; why?: string }
  | { state: 'stuck'; why?: string };
/** Its status as kept first (waiting for approval, declined, taken back, rejected, delivered: done for its asker and closed),
 *  then how its record is held (done, arrived, partly there), then a Want that waits (its asker over their room, with the room's
 *  numbers in bytes, AL02; the server short of space, AL03), then what the fetchers say of one wanted. */
export type RequestState =
  | { state: 'waiting_for_approval' | 'declined' | 'cancelled' | 'rejected' | 'delivered' | 'arrived' | 'done' | 'waiting_for_space' }
  | { state: 'waiting_for_room'; used: number; room: number }
  | { state: 'partly_there'; has: number; of: number }
  | { state: 'wanted'; lidarr?: LidarrWord; soulseek?: SoulseekWord };

/** Still open: asked for and not in the library yet, nor closed by an answer (a Want that waits is open, a delivered one is
 *  not). A closed one is wanted again by a new Want. */
const OPEN = new Set<RequestState['state']>(['waiting_for_approval', 'waiting_for_room', 'waiting_for_space', 'wanted', 'partly_there', 'arrived']);
export const isOpen = (s: RequestState | null | undefined): boolean => !!s && OPEN.has(s.state);

/** A request as the views read it: its state where the server worked one out (every list's), else (a request just made, Want's
 *  answer) its status as kept. */
export type Asked = {
  status: string;
  state?: RequestState | null;
  reason?: string | null;
  fetched?: string | null;
  at: number;
  /** When it was answered, or delivered (seconds). */
  decided_at?: number | null;
  hand_over_again?: boolean;
};

/** Whether a request is still open: by its state; one just made, by its status (it has no state yet). */
export const requestOpen = (r: Asked | null | undefined): boolean => !!r && (r.state ? isOpen(r.state) : r.status === 'pending' || r.status === 'approved');

/** How far a fetch has come, 0 to 1: Lidarr's or Soulseek's, the further; none while nothing fetches. */
export function fillOf(s: RequestState | null | undefined): number | null {
  if (s?.state !== 'wanted') return null;
  const lidarr = s.lidarr?.state === 'fetching' ? s.lidarr.done : null;
  const soulseek = s.soulseek?.state === 'fetching' ? s.soulseek.percent / 100 : null;
  return lidarr === null && soulseek === null ? null : Math.max(lidarr ?? 0, soulseek ?? 0);
}

/** What a Want that waits says, wherever it shows (RQ01.9: its row, and a story's Lidarr line, PB4): for its asker's room, for
 *  space, for its album's release. */
export const WAITS_FOR = { room: 'Waits for room', space: 'Waits: the server is short of space', release: 'Waits for its release' } as const;

/** Its state in words, as the Requests page says it: the server's state, never worked out again here. `said` says when. */
export function standing(r: Asked, said: (t: number) => string): string {
  const s = r.state;
  if (!s) return r.status === 'pending' ? `Waiting · ${said(r.at)}` : (r.fetched ?? 'Approved');
  switch (s.state) {
    case 'waiting_for_approval':
      return `Waiting · ${said(r.at)}`;
    case 'waiting_for_room': {
      // Its room's numbers where the server read them (none: both 0, and only that it waits is said).
      const line = s.used || s.room ? roomLine(s.used, s.room / GB) : null;
      return line ? `${WAITS_FOR.room} · ${line}` : WAITS_FOR.room;
    }
    case 'waiting_for_space':
      return WAITS_FOR.space;
    case 'delivered':
      return `Delivered · ${said(r.decided_at ?? r.at)}`;
    case 'declined':
      return r.reason ? `Declined: ${r.reason}` : 'Declined';
    case 'cancelled':
      return `Taken back · ${said(r.at)}`;
    case 'rejected':
      return 'Rejected';
    case 'done':
      return 'In the library';
    case 'arrived':
      return 'Arrived';
    case 'partly_there':
      return `Partly there · ${s.has} of ${s.of}`;
    case 'wanted': {
      const done = fillOf(s);
      if (done !== null) return `Being fetched · ${Math.round(done * 100)}%`;
      if (s.lidarr?.state === 'looking') return 'Lidarr is looking for it';
      const w = s.soulseek;
      if (w?.state === 'searching') return 'Soulseek is looking for it';
      if (w?.state === 'found') return 'Found on Soulseek';
      if (w?.state === 'waiting') return w.why || 'Not found yet';
      if (w?.state === 'stuck') return w.why || 'Stuck';
      return r.fetched ?? 'Approved';
    }
  }
}

/** Its row's tone: declined, rejected, stuck, or not handed over, said so. */
export function standingTone(r: Asked): 'bad' | null {
  const s = r.state?.state;
  const stuck = r.state?.state === 'wanted' && r.state.soulseek?.state === 'stuck';
  return s === 'declined' || s === 'rejected' || stuck || !!r.fetched?.startsWith('Not handed over') ? 'bad' : null;
}

/** What a card says of its request (Discover's): "Requested" while it waits for an answer; nothing once taken back, nor where
 *  its mark says it was not handed over; else its standing, never the fetcher's own words. */
export function cardStanding(r: Asked, said: (t: number) => string): string {
  if (r.state?.state === 'waiting_for_approval' || (!r.state && r.status === 'pending')) return 'Requested';
  if (r.state?.state === 'cancelled' || (!r.state && r.status === 'cancelled') || r.fetched?.startsWith('Not handed over')) return '';
  return standing({ ...r, fetched: null }, said);
}

/** A cover's mark for its request (lib/CoverStatus): coming (how far) once fetched or arriving, else wanted while open; none
 *  once in the library or closed. */
export function markOfState(r: Asked | null | undefined): { state: 'wanted' | 'downloading'; done?: number | null } | null {
  if (!r || !requestOpen(r)) return null;
  const s = r.state;
  if (s?.state === 'arrived' || s?.state === 'partly_there') return { state: 'downloading', done: 1 };
  const done = fillOf(s);
  return done !== null ? { state: 'downloading', done } : { state: 'wanted' };
}

/** Whether Take back is offered on a request (RF18, the review of run 13, RM3): its asker's own, or anyone's for those who answer
 *  requests, while it is pending or approved, as the server's `cancel` allows. A Want goes ahead with no one's yes since 0.17 (AL02),
 *  so one waiting for room or for space is approved, and its asker's to take back. */
export const mayTakeBack = (r: { status: string; user: string }, me: string | null | undefined, answers: boolean): boolean =>
  (r.status === 'pending' || r.status === 'approved') && (r.user === me || answers);

/** Hand over again (W5, G2): for those who answer requests, where the server says no fetcher has spoken of one wanted. */
export const handOverAgain = (r: Asked, answers: boolean) => answers && r.hand_over_again === true;
