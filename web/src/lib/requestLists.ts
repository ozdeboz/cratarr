// Requests' lists (decision 0070; the fetchers as siblings, decision 0099's amendment), pure: which lists there are and
// which one sees.
import type { IconName } from './Icon.svelte';

export type RequestsList = 'waiting' | 'mine' | 'lidarr' | 'soulseek';
/** `label`: the row on the Requests page; `name`: short, the panel's breadcrumb (the owner, 2026-10-26: Requests ›
 *  Queue, not Requests › Waiting for an answer; as Health's). */
export const REQUESTS_LISTS: { id: RequestsList; label: string; name: string; icon: IconName }[] = [
  { id: 'waiting', label: 'Queue', name: 'Queue', icon: 'inbox' },
  { id: 'mine', label: 'Your requests', name: 'Yours', icon: 'user' },
  // The fetchers, siblings (decision 0099's amendment; the owner, 2026-11-03: "they are still siblings from our point of
  // view"): each its own list where it is connected, the same shape: what it fetches now, what it has not found yet.
  // Lidarr's was "Monitor". Soulseek's ticks what is fetched: administrators' only.
  { id: 'lidarr', label: 'Lidarr', name: 'Lidarr', icon: 'download' },
  { id: 'soulseek', label: 'Soulseek', name: 'Soulseek', icon: 'download' },
];
/** The lists one sees: Queue only for those who answer requests; each fetcher's where it is connected, Soulseek's
 *  only for administrators. */
export const listsShown = (answers: boolean, admin: boolean, fetchers: readonly string[]) =>
  REQUESTS_LISTS.filter((l) => (l.id !== 'waiting' || answers) && (l.id !== 'lidarr' || fetchers.includes('lidarr')) && (l.id !== 'soulseek' || (admin && fetchers.includes('soulseek'))));
