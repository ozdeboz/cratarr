// Queue history (decision 0033, amended): the queues of the player that plays, the one that plays marked, newest first,
// to take up again where they were left, rename or delete. Cratarr keeps each player's (the app's own, a player driven
// from here, and since 0.12 a browser's or the desktop app's own from its reports): one list wherever it is read. A
// browser playing itself reads Cratarr's while it answers, and its own (localPlayer's `keptHere`) out of reach; its
// changes are made here, and Cratarr hears them in its reports.

import { getPlayer, playerCommand, type PlayerCommand, type QueueItem } from './api';
import { appInfo, inApp } from './app.svelte';
import { deleteLocal, keptHere, localQueues, local, renameLocal, switchLocal } from './localPlayer.svelte';
import { drivenName, playOn } from './playOn.svelte';
import { deviceName, target } from './playOnTarget.svelte';
import { reach } from './reach.svelte';

/** The list as last read; `error`: why it could not be read or changed, in plain words; `known`: read once (until then
 *  no "No queues yet"). */
export const queues = $state({ list: [] as QueueItem[], error: null as string | null, known: false });

/** The player whose queues these are: the one driven from here, else the app's own; null: this browser's own. */
function player(): string | null {
  return drivenName() ?? (inApp ? appInfo.player : null);
}
export const queuesPlayer = player;

/** This browser's own player as Cratarr knows it, while it answers: its list is read from there. */
const own = () => (target.connected && !reach.away ? deviceName() : null);

/** The list's version as Cratarr says it with each player, and this browser's: a change means read it again. */
export function queuesVersion(): string {
  const name = player();
  if (name === null) {
    const mine = own();
    const p = mine === null ? undefined : playOn.players.find((x) => x.name === mine);
    return `here:${keptHere.list.length}:${keptHere.id}:${local.queue.length}:${local.from}:${p?.queues_version ?? 0}`;
  }
  const p = playOn.players.find((x) => x.name === name);
  return `${name}:${p?.queues_version ?? 0}:${p?.queue_version ?? 0}`;
}

/** Reads the list again. */
export async function loadQueues() {
  const name = player();
  if (name === null) {
    const mine = own();
    let list = localQueues();
    // Cratarr's, unless it has none yet for this browser (its reports bring them) or did not answer.
    if (mine !== null) list = (await getPlayer(mine).then((p) => (p.queues?.length ? p.queues : null), () => null)) ?? list;
    queues.list = list;
    queues.error = null;
    queues.known = true;
    return;
  }
  try {
    queues.list = (await getPlayer(name)).queues ?? [];
    queues.error = null;
  } catch (e) {
    queues.error = e instanceof Error ? e.message : String(e);
  }
  queues.known = true;
}

async function command(c: PlayerCommand, here: () => void) {
  const name = player();
  try {
    if (name === null) here();
    else await playerCommand(name, c);
    queues.error = null;
  } catch (e) {
    queues.error = e instanceof Error ? e.message : String(e);
  }
  await loadQueues();
}

/** Takes up queue `id` where it was left (playing if it played, else paused there); the one that played is kept. */
export const switchQueue = (id: string) => command({ command: 'queue', id }, () => switchLocal(id));

/** Renames queue `id` (the one that plays too); a renamed one stays until deleted. */
export const renameQueue = (id: string, name: string) => command({ command: 'queue_rename', id, name }, () => renameLocal(id, name));

/** Takes queue `id` out of the list; not the one that plays. */
export const deleteQueue = (id: string) => command({ command: 'queue_delete', id }, () => deleteLocal(id));
