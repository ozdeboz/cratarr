// A request's state as the server works it out (W5: one state every view reads, `requests::RequestState`), never worked out
// again here from its status, its album or what Lidarr said. Pure.

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
/** Its status as kept first (waiting for approval, declined, taken back, rejected), then how its record is held (done,
 *  arrived, partly there), then what the fetchers say of one wanted. */
export type RequestState =
  | { state: 'waiting_for_approval' | 'declined' | 'cancelled' | 'rejected' | 'arrived' | 'done' }
  | { state: 'partly_there'; has: number; of: number }
  | { state: 'wanted'; lidarr?: LidarrWord; soulseek?: SoulseekWord };

/** Still open: asked for and not in the library yet, nor closed by an answer. A closed one is wanted again by a new Want. */
export const isOpen = (s: RequestState | null | undefined): boolean =>
  !!s && (s.state === 'waiting_for_approval' || s.state === 'wanted' || s.state === 'partly_there' || s.state === 'arrived');

/** A request as the views read it: its state where the server worked one out (every list's), else (a request just made, Want's
 *  answer) its status as kept. */
export type Asked = {
  status: string;
  state?: RequestState | null;
  reason?: string | null;
  fetched?: string | null;
  at: number;
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

/** Its state in words, as the Requests page says it: the server's state, never worked out again here. `said` says when. */
export function standing(r: Asked, said: (t: number) => string): string {
  const s = r.state;
  if (!s) return r.status === 'pending' ? `Waiting · ${said(r.at)}` : (r.fetched ?? 'Approved');
  switch (s.state) {
    case 'waiting_for_approval':
      return `Waiting · ${said(r.at)}`;
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

/** Hand over again (W5, G2): for those who answer requests, where the server says no fetcher has spoken of one wanted. */
export const handOverAgain = (r: Asked, answers: boolean) => answers && r.hand_over_again === true;
