// Requests (decision 0070) as the page and its panel share them (the owner, 2026-10-24: the page an index, as
// Downloads, Insights, Health and Settings; the lists in the panel): the account's requests and everyone's waiting ones,
// what Lidarr is looking for, and the actions on them.
import { cancelRequest, decideRequests, getRequests, getWatching, type AlbumRequest, type RequestsView, type Watching } from './api';
import type { IconName } from './Icon.svelte';
import { toast } from './toast.svelte';

/** The page's lists, each opened in the panel (`requests:<id>`). */
export type RequestsList = 'waiting' | 'mine' | 'lidarr';
export const REQUESTS_LISTS: { id: RequestsList; label: string; icon: IconName }[] = [
  { id: 'waiting', label: 'Waiting for an answer', icon: 'inbox' },
  { id: 'mine', label: 'Your requests', icon: 'user' },
  { id: 'lidarr', label: 'Lidarr monitor', icon: 'download' },
];

export const requests = $state({
  view: null as RequestsView | null,
  watching: null as Watching | null,
  error: null as string | null,
});

export const loadRequests = () =>
  getRequests().then(
    (v) => ((requests.view = v), (requests.error = null)),
    (e) => (requests.error = e instanceof Error ? e.message : String(e)),
  );

/** Lidarr asked, not watched: while the page or its panel shows, again every half minute (its queue moves). */
export const lookAtLidarr = () =>
  getWatching()
    .then((w) => (requests.watching = w))
    .catch(() => {});

/** Those who answer requests. */
export const answersRequests = () => !!requests.view?.may.manage_requests;

/** Requests answered, one or many; only what went wrong is said (the rows show the rest). */
export async function decide(ids: number[], approve: boolean, reason: string | null = null) {
  try {
    const out = await decideRequests(ids, approve, reason);
    const wrong = out.filter((o) => o.said.startsWith('Not handed over') || o.said.startsWith('Already') || o.said.includes('gone'));
    if (wrong.length) toast.show({ text: wrong.length > 1 ? `${wrong[0].said} (and ${wrong.length - 1} more)` : wrong[0].said, tone: 'error' });
  } catch (e) {
    toast.fail(e, approve ? 'Could not approve it' : 'Could not decline it');
  }
  loadRequests();
  lookAtLidarr();
}

export async function takeBack(r: AlbumRequest) {
  try {
    await cancelRequest(r.id);
  } catch (e) {
    toast.fail(e, 'Could not take it back');
  }
  loadRequests();
}
