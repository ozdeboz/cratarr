// Level the volume (decision 0035), this device's: in the app its own player's, here this browser's. The settings' rows
// (lib/settings/LevelItems) and the player's Level button (lib/NowPlaying) read and set it here.
import { appInfo, inApp, setDevice } from './app.svelte';
import { setLevelling } from './localPlayer.svelte';
import { prefs } from './prefs.svelte';

export const levelOn = () => (inApp ? appInfo.levelling : prefs.levelling);
export const setLevelOn = (on: boolean) => (inApp ? setDevice({ levelling: on }) : setLevelling(on));
export const levelUnknown = () => (inApp ? appInfo.levelUnknown : prefs.levelUnknown);
export const setLevelUnknown = (v: 'typical' | 'untouched') => (inApp ? setDevice({ levelUnknown: v }) : (prefs.levelUnknown = v));
/** How much of the Level button fills: the dB the playing track is turned down by, against LEVEL_FULL_DB. */
const LEVEL_FULL_DB = 12;
export const levelShare = (db: number) => Math.min(1, Math.max(0, -db / LEVEL_FULL_DB));
