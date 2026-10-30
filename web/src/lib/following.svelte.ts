// Following and queue requests (0.10; the owner, 2026-10-30: "This is a queue matter"): one's own player listening
// along to another user's (who lets others follow them), the tracks asked of the one followed, and those asked of
// one's own players, accepted (a thumbs up: into the queue where asked) or declined (a thumbs down: gone). No page of
// its own: the Play On sheet's foot (lib/OutputSheet) and the queue sheet's rows (lib/QueueSheet). A server before it
// has none of it (its calls answer 404): nothing of it shows then.
import { acceptRequest, ApiError, askForTrack, declineRequest, getFollowers, getLeaders, getQueueRequests, type Follower, type FollowLeader, type FollowView, type PlayOnPlayer, type QueueRequest, type QueueTrack } from './api';
import { port } from './player/port.svelte';
import { drivenName, playOn, selfName } from './playOn.svelte';
import { toast } from './toast.svelte';
import { can } from './capabilities.svelte';

export const follow = $state({
  /** The server keeps following (null: not asked yet). */
  served: null as boolean | null,
  followers: [] as Follower[],
  leaders: [] as FollowLeader[],
  /** The tracks asked of each of one's own players, pending, oldest first. */
  asked: {} as Record<string, QueueRequest[]>,
  /** Accepted here a moment ago: shown as rows of the queue until the player's report has them (lib/queueRequests). */
  accepted: [] as { r: QueueRequest; before: number; at: number }[],
  /** One's own asked of the one followed, as the server last said. */
  mine: [] as QueueRequest[],
});

/** Following offered: as the server's list says (0.11, lib/capabilities), else as its calls answered (a 404: none). */
export const followOffered = () => can('follow') ?? follow.served;
/** Asking for tracks (queue requests) offered, likewise. */
export const requestsOffered = () => can('queue_requests') ?? follow.served;

const served = (e: unknown) => {
  if (e instanceof ApiError && e.status === 404) follow.served = false;
};

/** A player's view as Play On lists it, with how following and asking stand on it. */
export const viewOf = (name: string | null) => (name ? (playOn.players.find((p) => p.name === name) as (PlayOnPlayer & FollowView) | undefined) : undefined);
/** The player that plays what this page shows, where it is one of Cratarr's own (here, or one driven). */
export const playerShown = () => (port().holdsQueue ? selfName() : drivenName());
/** Whom this device's player follows now (its Next, Previous and queue edits are then the one followed's). */
export const followingNow = () => viewOf(selfName())?.following ?? null;
/** How many tracks are asked of the player shown, where it is one's own. */
export function askedOf(name: string | null): number {
  const v = viewOf(name);
  if (!v || v.mine === false) return 0;
  return v.queue_requests ?? follow.asked[name!]?.length ?? 0;
}

/** Who follows and whom one may follow (the Play On sheet opening). */
export async function loadFollow() {
  try {
    const [followers, leaders] = await Promise.all([getFollowers(), getLeaders()]);
    Object.assign(follow, { served: true, followers, leaders });
  } catch (e) {
    served(e);
  }
}

/** The tracks asked of one of one's own players (the queue sheet showing it). */
export async function loadAsked(player: string) {
  try {
    follow.asked[player] = await getQueueRequests(player);
    follow.served = true;
  } catch (e) {
    served(e);
  }
}

/** A thumbs up: into the queue where it was asked for, a row of it at once (`before`: how often the queue holds its
 *  track now, so the report that has it once more takes the row's place). */
export async function accept(r: QueueRequest, before: number) {
  follow.asked[r.player] = (follow.asked[r.player] ?? []).filter((x) => x.id !== r.id);
  follow.accepted = [...follow.accepted, { r, before, at: Date.now() }];
  try {
    await acceptRequest(r.id);
  } catch (e) {
    follow.accepted = follow.accepted.filter((a) => a.r.id !== r.id);
    follow.asked[r.player] = [...(follow.asked[r.player] ?? []), r];
    toast.fail(e, "It didn't go into the queue");
  }
}

/** A thumbs down: gone from the queue at once (the one who asked hears of it from the server). */
export async function decline(r: QueueRequest) {
  follow.asked[r.player] = (follow.asked[r.player] ?? []).filter((x) => x.id !== r.id);
  try {
    await declineRequest(r.id);
  } catch (e) {
    follow.asked[r.player] = [...(follow.asked[r.player] ?? []), r];
    toast.fail(e, "It wasn't declined");
  }
}

/** Asks the one followed for these tracks, next or at the end of their queue; whether any went in at once. */
export async function askFor(tracks: QueueTrack[], how: 'next' | 'last'): Promise<void> {
  const to = followingNow();
  if (!to || !tracks.length) return;
  let at = 0;
  for (const t of tracks) {
    const r = await askForTrack(to, t, how);
    if (r.state === 'accepted') at++;
  }
  toast.show({ text: at === tracks.length ? `In ${to.user}'s queue.` : `Asked ${to.user}.`, tone: 'done' });
  void loadMine();
}

/** Who asked, as a row's third line says it. */
export const askerName = (r: QueueRequest) => ('user' in r.from ? r.from.user : r.from.guest);

/** One's own asked, read again; an answer to one (while this page is in front) as a toast. */
export async function loadMine() {
  try {
    const r = await fetch('/api/queue-requests/mine', { cache: 'no-store' });
    if (!r.ok) return;
    const now = (await r.json()) as QueueRequest[];
    if (typeof document !== 'undefined' && document.visibilityState === 'visible') {
      for (const was of follow.mine) {
        const is = now.find((x) => x.id === was.id);
        if (was.state === 'pending' && is && is.state !== 'pending') toast.show({ text: `${is.state === 'accepted' ? 'Accepted' : 'Declined'}: ${is.track.name}.` });
      }
    }
    follow.mine = now;
  } catch {
    /* asked again in a while */
  }
}

/** One's own still waiting, for the queue sheet ("Waiting for Ann"). */
export const waiting = () => follow.mine.filter((r) => (r.state ?? 'pending') === 'pending');

if (typeof window !== 'undefined') {
  // While one's own wait for an answer: asked again every so often (the server also tells the app's players).
  setInterval(() => {
    if (waiting().length && document.visibilityState === 'visible') void loadMine();
  }, 15_000);
}
