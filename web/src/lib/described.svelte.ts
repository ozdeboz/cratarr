// What a track is, as the server says (decision 0033, amended 2026-10-25: the queue holds identities, Cratarr
// describes them; the owner chose this over patching the menu: "the robust one"). This page's own queue (lib/localPlayer)
// is described here as it changes: each track of Cratarr's own library or of Bandcamp the server has not described
// yet is asked for (a few hundred at a time) and its name, artist, album, album id, length and whether it can go in a
// playlist are the server's, whatever list it was started from. Other servers' tracks stay as they came. A server
// before describe (0.9.5) answers 404: then nothing is asked again, and reports keep carrying the details.
import { can } from './capabilities.svelte';
import { untrack } from 'svelte';
import { describeTracks, missingCall, type DescribedTrack } from './api';
import { local, type LocalTrack } from './localPlayer.svelte';

/** Whether the server describes tracks: null until the first answer. */
export const describing = $state({ server: null as boolean | null });

const MOST = 500;
const key = (t: { source: string; id: string }) => `${t.source}/${t.id}`;
/** The server's descriptions, by `source/id`. */
const known = new Map<string, DescribedTrack>();
const asked = new Set<string>();
const ours = (t: LocalTrack) => t.source === 'cratarr' || t.source === 'bandcamp';

/** The server's description over the track (its place in the queue, `entry`, `lane` and `n`, are the queue's). */
function apply(t: LocalTrack, d: DescribedTrack) {
  if (!d.found) return;
  if (t.name !== d.name) t.name = d.name;
  if (t.artist !== d.artist) t.artist = d.artist;
  if (t.album !== d.album) t.album = d.album;
  if (t.album_id !== d.album_id) t.album_id = d.album_id;
  if (d.duration_secs !== null && t.duration_secs !== d.duration_secs) t.duration_secs = d.duration_secs;
  if (t.addable !== d.addable) t.addable = d.addable;
}

async function describe(queue: LocalTrack[]) {
  for (const t of queue) {
    const d = known.get(key(t));
    if (d) apply(t, d);
  }
  // Not asked of a server whose list says it cannot (0.11, lib/capabilities: describe), nor of one found without the call.
  if (describing.server === false || can('describe') === false) return;
  const wanted = [...new Map(queue.filter((t) => ours(t) && !asked.has(key(t))).map((t) => [key(t), { source: t.source, id: t.id }])).values()];
  for (let at = 0; at < wanted.length; at += MOST) {
    const part = wanted.slice(at, at + MOST);
    for (const t of part) asked.add(key(t));
    try {
      const { tracks } = await describeTracks(part);
      describing.server = true;
      for (const d of tracks) known.set(key(d), d);
    } catch (e) {
      // Asked again with the next change, unless the server has no such call.
      for (const t of part) asked.delete(key(t));
      if (missingCall(e)) describing.server = false;
      return;
    }
    // The queue as it is now: what was asked may have moved or gone meanwhile.
    for (const t of local.queue) {
      const d = known.get(key(t));
      if (d) apply(t, d);
    }
  }
}

$effect.root(() => {
  $effect(() => {
    // Read again when which tracks the queue holds changes, not when a description lands in one.
    void local.queue.map(key).join('\n');
    untrack(() => void describe(local.queue));
  });
});
