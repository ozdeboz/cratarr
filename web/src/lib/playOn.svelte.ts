// Cratarr's own players (Play On, decision 0031): the Android app on a tablet or a phone. The live channel says
// what each plays (`players`); the page can play on one (prefs.output `player:<name>`), which it then drives as it
// drives the local player (localPlayer.svelte.ts, `drive`): the queue, the now-playing view and the controls are the
// same, the music plays there.

import { getPlayer, getPlayers, playerCommand, type PlayOnPlayer } from './api';
import { appInfo, inApp, toApp } from './app.svelte';
import { on } from './events.svelte';
import { drive, emptyHere, reportedBy, type LocalTrack } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';

/** `self`: the name this page is a player under, in a browser (lib/playOnTarget); null in the app or before. */
export const playOn = $state({ players: [] as PlayOnPlayer[], self: null as string | null });

/** The player chosen as the output, by name; null when the output is not one of Cratarr's players. */
export const drivenName = () => (prefs.output.startsWith('player:') ? prefs.output.slice('player:'.length) : null);

/** The players that can be chosen: connected now, and not the app this page runs in (that one is "This device"). */
export const choosable = () =>
  playOn.players.filter((p) => p.online && !(inApp && p.name === appInfo.player) && !(!inApp && p.name === (playOn.self ?? prefs.browserName)));

/** While this device follows a group (decision 0037; a page in a browser too, amended 2026-10-23): the group's leader.
 *  The device plays no queue of its own then, so the page shows the leader's and its controls act there (the owner,
 *  2026-10-16: the follower showed no player at all). Null otherwise. */
export const followedLeader = () => {
  const me = selfName();
  if (prefs.output !== 'local' || !me) return null;
  const group = playOn.players.find((p) => p.name === me)?.group;
  return group && group !== me ? group : null;
};

/** This device's own player: the app's, or this page's in a browser. */
export const selfName = () => (inApp ? appInfo.player : playOn.self);

/** The player whose reports the page shows and drives: the one chosen, else the leader this device follows. */
const shownName = () => drivenName() ?? followedLeader();
/** What the local player was last told to drive. */
let driving: string | null = null;
/** What the app was last told its screen drives. */
let told: string | null | undefined;
/** In the app, the player chosen (null: its own) to the app, so Android's media controls show that one meanwhile (the
 *  owner, 2026-10-23). Before the app's player is asked for its state again (`hello`): it answers for its own then. */
function tellApp() {
  const d = drivenName();
  if (!inApp || d === told) return;
  told = d;
  toApp({ type: 'drive', player: d });
}
function driveShown() {
  tellApp();
  const name = shownName();
  if (name === driving) return;
  const was = driving;
  driving = name;
  shownVersion = -1;
  drive(name);
  // Out of the group again: the app's own player says what it holds now.
  if (!name && was && inApp) toApp({ type: 'hello' });
}

/** Music started on this device's own player while its screen drove another (Play On from elsewhere; decision 0033
 *  amended 2026-10-23): the screen turns here, and the other plays on, no longer driven from here. */
export function hereAgain() {
  if (prefs.output === 'local') return;
  prefs.output = 'local';
  driveShown();
  if (!inApp) emptyHere();
}

/** Whether the app's own player played at its last report: one that starts while the screen drives another turns the
 *  screen here (`hereAgain`). */
let ownPlayed: boolean | null = null;
function watchOwn(players: PlayOnPlayer[]) {
  if (!inApp || !appInfo.player) return;
  const own = players.find((p) => p.name === appInfo.player);
  const now = !!own?.online && own.playing;
  if (now && ownPlayed === false) hereAgain();
  ownPlayed = now;
}

/** The one of Cratarr's players the page plays on: the one chosen, or this device's own (the app's, or this page's)
 *  playing here. */
export const head = () => drivenName() ?? (prefs.output === 'local' ? selfName() : null);

/** The players of `name`'s group (decision 0037), its leader first; none while it plays alone. */
export function groupOf(name: string | null): string[] {
  const leader = name ? playOn.players.find((p) => p.name === name)?.group : null;
  if (!leader) return [];
  const rest = playOn.players.filter((p) => p.group === leader && p.name !== leader).map((p) => p.name);
  return [leader, ...rest];
}

/** Where the music plays, as the device button says it: a group by its players ("Salon tablet + Phone"). */
export function outputName(): string {
  const shown = (n: string) => (n === selfName() ? 'This device' : n);
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
  watchOwn(players);
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
  tellApp();
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
