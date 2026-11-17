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
