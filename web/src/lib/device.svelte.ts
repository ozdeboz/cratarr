// The settings both kinds of device have, each read and changed in one way (lib/deviceSettings: one owner each): Start
// and stop with the tape, its owner's copy (the app's in the app, this browser's in one); levelling is lib/level's.
import { appInfo, inApp, setDevice } from './app.svelte';
import { ownCopy } from './deviceSettings';
import { prefs, type StartStop } from './prefs.svelte';

/** This device's Start and stop and its tape. */
export const startStopNow = (): StartStop => ownCopy(inApp, appInfo.startStop, prefs.startStop);

/** Changes it where its owner keeps it: in the app shown at once, the app's answer then saying it as kept. */
export function setStartStopNow(change: Partial<StartStop>) {
  const next = { ...startStopNow(), ...change };
  if (inApp) {
    appInfo.startStop = next;
    setDevice({ startStop: next });
  } else prefs.startStop = next;
}
