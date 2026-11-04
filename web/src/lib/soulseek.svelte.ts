// Soulseek in Cratarr (decision 0099 and its amendment; the owner, 2026-11-03: on the Requests page, a fetcher beside
// Lidarr, never reading Lidarr's Wanted): the requests approved in Cratarr, as it searches and fetches them, read while the list shows (often while a search or a fetch is under way), and the
// administrator's calls on them. Administrators' only, as the server's routes are.
import { approveSoulseek, clearSoulseekPlaced, dropSoulseekItem, getSoulseek, soulseekNow } from './api';
import type { SoulseekView } from './soulseekRules';
import { toast } from './toast.svelte';

export const soulseek = $state({ view: null as SoulseekView | null, error: null as string | null });

const took = (v: SoulseekView) => ((soulseek.view = v), (soulseek.error = null));

export function loadSoulseek() {
  return getSoulseek().then(took, (e) => (soulseek.error = e instanceof Error ? e.message : 'Unexpected error'));
}
/** The ticked found records fetched, each from its picked folder (else the best). */
export const fetchFound = (items: { id: string; folder?: number }[]) => approveSoulseek(items).then(took, (e) => toast.fail(e, "They weren't fetched"));
/** Searched now, whatever its wait. */
export const searchSoulseekNow = (id: string) => soulseekNow(id).then(took, (e) => toast.fail(e, "It isn't being searched"));
/** Out of the list. */
export const dropSoulseek = (id: string) => dropSoulseekItem(id).then(took, (e) => toast.fail(e, "It wasn't taken out"));
/** Those placed in Incoming cleared from the list. */
export const clearPlaced = () => clearSoulseekPlaced().then(took, (e) => toast.fail(e, "They weren't cleared"));
