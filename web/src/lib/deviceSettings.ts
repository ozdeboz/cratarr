// This device's settings, each with one owner (the architecture audit, 2026-10-29: device settings with one owner;
// 0.10). In the app the app keeps its player's (sent with `device-set`, every one of them in its answer, lib/app),
// in a browser this browser does (lib/prefs, kept here); a player's speaker delay and its sharing are Cratarr's, for
// any kind. What one owner keeps is read and changed in one way (lib/device.svelte for the two that both kinds have:
// Start and stop, and levelling), never from a second copy: the page's copy of Start and stop or levelling in the app
// is not this device's setting. Pure, so the tests take it as it is.

export type Owner = 'app' | 'page' | 'server';
export type DeviceSetting = {
  id: string;
  label: string;
  /** Who keeps it in the app, and in a browser (null: a browser has none). */
  app: Owner;
  browser: Owner | null;
  /** The app's names for it in `device-set` and in its answer (lib/app's setDevice). */
  appKeys?: readonly string[];
};

export const DEVICE_SETTINGS: readonly DeviceSetting[] = [
  { id: 'name', label: 'This device’s name', app: 'app', browser: 'page', appKeys: ['player'] },
  { id: 'startStop', label: 'Start and stop, the tape', app: 'app', browser: 'page', appKeys: ['startStop'] },
  { id: 'levelling', label: 'Level the volume', app: 'app', browser: 'page', appKeys: ['levelling'] },
  { id: 'levelUnknown', label: 'Tracks not measured yet', app: 'app', browser: 'page', appKeys: ['levelUnknown'] },
  { id: 'addresses', label: 'Network', app: 'app', browser: null, appKeys: ['public', 'server', 'local', 'preferLocal'] },
  { id: 'stayReady', label: 'Stay ready', app: 'app', browser: null, appKeys: ['stayReady'] },
  { id: 'sync', label: 'Sync', app: 'app', browser: null, appKeys: ['downloadQuality', 'downloadOverMobile', 'syncBudgetGb', 'syncOnlyCharging'] },
  { id: 'output', label: 'Audio output', app: 'app', browser: null, appKeys: ['output', 'ordered', 'order', 'forget'] },
  { id: 'visualsAuto', label: 'Visuals by themselves', app: 'page', browser: 'page' },
  { id: 'delay', label: 'Speaker delay', app: 'server', browser: 'server' },
  { id: 'shared', label: 'Shared with everyone', app: 'server', browser: 'server' },
];

/** Who keeps setting `id` on this kind of device (null: it has none). */
export function ownerOf(id: string, inApp: boolean): Owner | null {
  const s = DEVICE_SETTINGS.find((x) => x.id === id);
  if (!s) return null;
  return inApp ? s.app : s.browser;
}

/** The one copy to read: the owner's of the two the page may hold (the app's answer, this browser's own). */
export const ownCopy = <T>(inApp: boolean, app: T, page: T): T => (inApp ? app : page);
