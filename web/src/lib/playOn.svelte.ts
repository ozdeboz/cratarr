// Cratarr's own players (Play On, decision 0031): the Android app on a tablet or a phone. The live channel says
// what each plays (`players`); the page can play on one (prefs.output `player:<name>`), which it then drives as it
// drives the local player (localPlayer.svelte.ts, `drive`): the queue, the now-playing view and the controls are the
// same, the music plays there.

import { untrack } from 'svelte';
import { getPlayer, getPlayers, playerCommand, type PlayOnPlayer } from './api';
import { appInfo, inApp, onSeedTarget, toApp } from './app.svelte';
import { outputOfTarget, targetOfOutput } from './player/commands';
import { on } from './events.svelte';
import { drive, emptyHere, reportedBy, type LocalTrack } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';
import { measure, speedOf, type Measured } from './drivenCourse';

/** `self`: the name this page is a player under, in a browser (lib/playOnTarget); null in the app or before. `known`:
 *  the players have been read once (until then no "None yet"). */
export const playOn = $state({ players: [] as PlayOnPlayer[], self: null as string | null, known: false });

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
function driveShown() {
  const name = shownName();
  if (name === driving) return;
  const was = driving;
  driving = name;
  shownVersion = -1;
  measured = null;
  drive(name);
  // Out of the group again: the app's own player says what it holds now.
  if (!name && was && inApp) toApp({ type: 'hello' });
}

/** In a browser: music sent to this page while its screen drove another (Play On from elsewhere; decision 0033
 *  amended 2026-10-23) turns the screen here, and the other plays on, no longer driven from here. In the app the app
 *  does this itself (decision 0062: sounding here means controls here) and the page follows (`fromApp`). */
export function hereAgain() {
  if (inApp || prefs.output === 'local') return;
  prefs.output = 'local';
  driveShown();
  emptyHere();
}

/** The page's output for an app's target (decision 0062), and back (lib/player/commands). */
const outputOf = outputOfTarget;
export const targetOf = targetOfOutput;

/** Points this device's controls at `output`: in the app it asks the app, which owns it (decision 0062) and answers
 *  with what it decided (shown meanwhile); in a browser the page owns it. */
export function pointAt(output: typeof prefs.output) {
  prefs.output = output;
  if (inApp && appInfo.target !== null) toApp({ type: 'target', to: targetOf(output) });
}

/** In the app, the app's target as the page's output: as it opens, and on every change (its own player started, a
 *  restart), whatever the page last thought. */
function fromApp(target: string) {
  const out = outputOf(target);
  // An app still pointed at the media server's player (gone in 0.12) is pointed here, so the two agree.
  if (target === 'server' && appInfo.target !== null) toApp({ type: 'target', to: 'self' });
  if (prefs.output === out) return;
  prefs.output = out;
  follow(drivenName());
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
/** The driven player's last place and the speed measured from it (lib/drivenCourse). */
let measured: Measured | null = null;
/** What a player's report may say beyond the view's fields where the server passes it on: its speed and how fast that
 *  changes (0075, as the app says them), and its hold's landing; a server that does not, leaves them out. */
type Moving = { speed_x?: number; accel_x?: number; landing?: { index: number; position_ms: number; top_x?: number } | null };

function take(players: PlayOnPlayer[]) {
  playOn.players = players;
  playOn.known = true;
  driveShown();
  const name = shownName();
  const p = name ? players.find((x) => x.name === name) : undefined;
  if (!p) return;
  const sleep = p.sleep ? { atMs: p.sleep.at_ms, endOfTrack: p.sleep.end_of_track } : null;
  const m = p as PlayOnPlayer & Moving;
  // Counted on from when its place was so, at its speed (the owner, 2026-10-30: through a wind the tablet glided to each
  // report and stood, while the ThinkPad wound on): as it says it, else measured between its reports.
  measured = measure(measured, { index: p.index, pos: p.position_ms / 1000, at: p.position_at });
  const speedX = speedOf(m.speed_x, p.rewinding ?? false, p.playing, measured.v);
  const l = m.landing;
  reportedBy({
    // From when it was first seen there (a view sent again carries a later stamp for the same place).
    ...(speedX === undefined ? {} : { speedX, accelX: typeof m.accel_x === 'number' ? m.accel_x : 0, deckAtMs: measured.seen.at }),
    // Its hold's landing, where the server passes it on: the look-ahead and the hold's zoom as the player has them.
    ...(l === undefined ? {} : { landing: l ? { index: l.index, positionMs: l.position_ms, topX: l.top_x } : null }),
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
    // How far it turns the playing track down: the Level button's fill and the waveform, as it hears it.
    levelDb: p.level_db ?? 0,
    direction: p.direction ?? 'R',
    // Winding there: the waveform follows its position as it reports it (the owner, 2026-10-27).
    rewinding: p.rewinding ?? false,
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
  if (inApp) {
    onSeedTarget(() => {
      appInfo.target = targetOf(prefs.output);
      toApp({ type: 'target', to: appInfo.target });
    });
    $effect.root(() => {
      $effect(() => {
        const t = appInfo.target;
        if (t !== null) untrack(() => fromApp(t));
      });
    });
  }
  on<PlayOnPlayer[]>('players', take);
  getPlayers()
    .then((r) => take(r.players))
    .catch(() => {
      /* the live channel brings them */
    });
  // A page opened with a player chosen before: drive it from the start.
  driveShown();
}
