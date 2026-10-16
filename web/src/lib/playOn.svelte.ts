// Cratarr's own players (Play On, decision 0031): the Android app on a tablet or a phone. The live channel says
// what each plays (`players`); the page can play on one (prefs.output `player:<name>`), which it then drives as it
// drives the local player (localPlayer.svelte.ts, `drive`): the queue, the now-playing view and the controls are the
// same, the music plays there.

import { getPlayer, getPlayers, playerCommand, type PlayOnPlayer } from './api';
import { appInfo, inApp, toApp } from './app.svelte';
import { on } from './events.svelte';
import { drive, reportedBy, type LocalTrack } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';

export const playOn = $state({ players: [] as PlayOnPlayer[] });

/** The player chosen as the output, by name; null when the output is not one of Cratarr's players. */
export const drivenName = () => (prefs.output.startsWith('player:') ? prefs.output.slice('player:'.length) : null);

/** The players that can be chosen: connected now, and not the app this page runs in (that one is "This device"). */
export const choosable = () => playOn.players.filter((p) => p.online && !(inApp && p.name === appInfo.player));

/** In the app, while this device follows a group (decision 0037): the group's leader. The device plays no queue of its
 *  own then, so the page shows the leader's and its controls act there (the owner, 2026-10-16: the follower showed no
 *  player at all). Null otherwise. */
export const followedLeader = () => {
  if (!inApp || prefs.output !== 'local' || !appInfo.player) return null;
  const group = playOn.players.find((p) => p.name === appInfo.player)?.group;
  return group && group !== appInfo.player ? group : null;
};

/** The player whose reports the page shows and drives: the one chosen, else the leader this device follows. */
const shownName = () => drivenName() ?? followedLeader();
/** What the local player was last told to drive. */
let driving: string | null = null;
function driveShown() {
  const name = shownName();
  if (name === driving) return;
  const was = driving;
  driving = name;
  shownVersion = -1;
  drive(name);
  // Out of the group again: the app's own player says what it holds now.
  if (!name && was && inApp) toApp({ type: 'hello' });
}

/** The one of Cratarr's players the page plays on: the one chosen, or the app this page runs in playing here. */
export const head = () => drivenName() ?? (inApp && prefs.output === 'local' ? appInfo.player : null);

/** The players of `name`'s group (decision 0037), its leader first; none while it plays alone. */
export function groupOf(name: string | null): string[] {
  const leader = name ? playOn.players.find((p) => p.name === name)?.group : null;
  if (!leader) return [];
  const rest = playOn.players.filter((p) => p.group === leader && p.name !== leader).map((p) => p.name);
  return [leader, ...rest];
}

/** Where the music plays, as the device button says it: a group by its players ("Salon tablet + Phone"). */
export function outputName(): string {
  const shown = (n: string) => (inApp && n === appInfo.player ? 'This device' : n);
  const g = groupOf(head());
  return g.length > 1 ? g.map(shown).join(' + ') : (drivenName() ?? 'This device');
}

/** A player held in the device sheet: into the group of the one played on, or out of it. */
export async function toggleGroup(name: string) {
  const h = head();
  if (!h || name === h) return;
  if (groupOf(h).includes(name)) await playerCommand(name, { command: 'leave' });
  else await playerCommand(name, { command: 'join', to: h });
}

/** The group played on ends (a tap plays on one player alone): its players but the leader leave it. */
export async function endGroup() {
  const [, ...rest] = groupOf(head());
  for (const n of rest) await playerCommand(n, { command: 'leave' });
}

/** The queue version shown, per player: a newer one is read whole. */
let shownVersion = -1;

function take(players: PlayOnPlayer[]) {
  playOn.players = players;
  driveShown();
  const name = shownName();
  const p = name ? players.find((x) => x.name === name) : undefined;
  if (!p) return;
  const sleep = p.sleep ? { atMs: p.sleep.at_ms, endOfTrack: p.sleep.end_of_track } : null;
  reportedBy({
    index: p.index,
    positionMs: p.position_ms,
    durationMs: p.duration_ms ?? -1,
    paused: !p.playing,
    error: null,
    repeat: p.repeat,
    sleep,
    radio: p.radio ?? false,
    radioBy: p.radio_by,
    radioCount: p.radio_count,
    skipFavourites: p.skip_favourites,
    autoplay: p.autoplay ?? null,
  });
  if (p.queue_version !== shownVersion) {
    const version = p.queue_version;
    getPlayer(p.name)
      .then((full) => {
        if (shownName() !== full.name) return;
        shownVersion = version;
        reportedBy({
          queue: (full.queue ?? []) as LocalTrack[],
          index: full.index,
          positionMs: full.position_ms,
          durationMs: full.duration_ms ?? -1,
          paused: !full.playing,
          error: null,
        });
      })
      .catch(() => {
        /* asked again with the next change */
      });
  }
}

/** Plays on player `name` from now on (or here again: `null`), as the output says. */
export function follow(name: string | null) {
  driving = name;
  drive(name);
  shownVersion = -1;
  take(playOn.players);
}

if (typeof window !== 'undefined') {
  on<PlayOnPlayer[]>('players', take);
  getPlayers()
    .then((r) => take(r.players))
    .catch(() => {
      /* the live channel brings them */
    });
  // A page opened with a player chosen before: drive it from the start.
  driveShown();
}
