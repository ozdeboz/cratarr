// Level the volume (decision 0035), this device's: in the app its own player's, here this browser's. The settings' rows
// (lib/settings/LevelItems) and the player's Level button (lib/NowPlaying) read and set it here.
// The player's Level button sets the player that plays (the owner, 2026-10-27): one of Cratarr's players driven from
// here over Play On (its `level` command; its report says how it stands), and a group as one: every player in it.
import { playerCommand } from './api';
import { appInfo, inApp, setDevice } from './app.svelte';
import { ownCopy } from './deviceSettings';
import { setLevelling } from './localPlayer.svelte';
import { drivenName, groupOf, head, outputName, playOn, selfName } from './playOn.svelte';
import { prefs } from './prefs.svelte';
import { toast } from './toast.svelte';

export const levelOn = () => ownCopy(inApp, appInfo.levelling, prefs.levelling);
export const setLevelOn = (on: boolean) => (inApp ? setDevice({ levelling: on }) : setLevelling(on));
export const levelUnknown = () => ownCopy(inApp, appInfo.levelUnknown, prefs.levelUnknown);
export const setLevelUnknown = (v: 'typical' | 'untouched') => (inApp ? setDevice({ levelUnknown: v }) : (prefs.levelUnknown = v));
/** Whose levelling the player's Level button sets: a group's players (leader first), else the one driven, else this
 *  device; `label` names it in its sheet, as the device button under the player does. */
export function levelTarget(): { players: string[]; label: string } {
  const g = groupOf(head());
  const d = drivenName();
  return { players: g.length > 1 ? g : d ? [d] : [], label: outputName() };
}
/** A player's own report of it (one that says nothing: off). */
const reported = (name: string) => playOn.players.find((p) => p.name === name);
/** Whether the player playing levels (a group: as its leader does). */
export function playingLevelOn(): boolean {
  const { players } = levelTarget();
  if (!players.length || players[0] === selfName()) return levelOn();
  return reported(players[0])?.levelling ?? false;
}
export function playingLevelUnknown(): 'typical' | 'untouched' {
  const { players } = levelTarget();
  if (!players.length || players[0] === selfName()) return levelUnknown();
  return reported(players[0])?.level_unknown === 'untouched' ? 'untouched' : 'typical';
}
/** Sets the player playing (each of a group's), this device itself directly. */
function toPlaying(command: { on?: boolean; unknown?: 'typical' | 'untouched' }, here: () => void) {
  const { players } = levelTarget();
  if (!players.length) return here();
  for (const name of players) {
    if (name === selfName()) {
      here();
      continue;
    }
    // Shown at once; its next report confirms it.
    const p = reported(name);
    if (p && command.on !== undefined) p.levelling = command.on;
    if (p && command.unknown) p.level_unknown = command.unknown;
    playerCommand(name, { command: 'level', ...command }).catch((e) => toast.fail(e, `${name} did not take it`));
  }
}
export const setPlayingLevelOn = (on: boolean) => toPlaying({ on }, () => setLevelOn(on));
export const setPlayingLevelUnknown = (v: 'typical' | 'untouched') => toPlaying({ unknown: v }, () => setLevelUnknown(v));
/** How much of the Level button fills: the dB the playing track is turned down by, against LEVEL_FULL_DB. */
const LEVEL_FULL_DB = 12;
export const levelShare = (db: number) => Math.min(1, Math.max(0, -db / LEVEL_FULL_DB));
