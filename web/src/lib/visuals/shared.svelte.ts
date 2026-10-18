// The visuals shared by a sync play group (decision 0046, amended; the server keeps each group's settings: claude-5f's
// /api/groups/{group}/visuals). Everything that shapes the picture is shared, so a slider moved on one device moves on
// all of them: the look and its pins, colours, intensity, flash, speed, the grain, the routes and each band's times.
// What each device can afford (points, steps, resolution, frame rate) and its brightness (a bright room, a dark one)
// stay its own.
// A device joining a group adopts the group's settings (a group with none yet takes the joiner's); one leaving keeps
// what it was showing. What the owner changes goes to the server as it is made, a drag's many changes once every 150 ms
// and only the settings changed (the server merges them, the routes route by route, so two people moving different
// sliders do not undo each other), and comes back to every device on the live channel's `visuals` event; a device
// knows its own changes by `writer` and takes none older than the last it took. A track's preset and the music's
// moments are not sent: every device works out the same ones.
// The saved looks (snapshots, on the server for every device) hold the same settings.

import { ApiError, getGroupVisuals, putGroupVisuals } from '../api';
import { on } from '../events.svelte';
import { head, playOn } from '../playOn.svelte';
import { PRESET_KEYS } from './flame';
import { keptFrom, saveVisualSettings, visualSettings, type VisualSettings } from './settings.svelte';

/** The settings shared, and saved in a snapshot. */
export const SHARED_KEYS = [
  ...PRESET_KEYS,
  'pins',
  'colours',
  'intensity',
  'flash',
  'speed',
  'smoothing',
  'floor',
  'depth',
  'routes',
  'lighting',
  'evolution',
  'continuity',
] as const satisfies readonly (keyof VisualSettings)[];
export type Shared = Partial<Record<(typeof SHARED_KEYS)[number], unknown>>;
/** The shared settings' form, kept with a snapshot so a later one can be read: 1 since 2026-10-18. */
export const SHARED_V = 1;

/** This page, as its changes come back from the server. */
const writer = (globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`).slice(0, 64);

/** The shared settings as they are now. */
export function sharedNow(): Shared {
  return Object.fromEntries(SHARED_KEYS.map((k) => [k, $state.snapshot(visualSettings[k])]));
}

/** Shared settings into this device's (each brought into its range; the routes as given, others as they were not). */
export function applyShared(settings: Record<string, unknown>) {
  const clean = keptFrom({ ...$state.snapshot(visualSettings), ...settings, pins: settings.pins ?? visualSettings.pins });
  for (const k of SHARED_KEYS) if (k in settings) (visualSettings as Record<string, unknown>)[k] = clean[k];
  saveVisualSettings();
}

// The group followed (its leader's name), the last version taken, and the changes waiting to be sent.
let group: string | null = null;
let version = 0;
// (and the ones on their way)
let outgoing: Shared = {};
let sending: Shared = {};
let pending: ReturnType<typeof setTimeout> | undefined;

/** The group this page's player is in: the app's own, or the one the page plays on. */
const groupNow = () => {
  const name = head();
  return (name && playOn.players.find((p) => p.name === name)?.group) || null;
};

/** Follows the group the page's player is in while the visuals show; returns the function that stops it. */
export function followGroup(): () => void {
  const stopEvents = on<{ group: string; version: number; settings: Record<string, unknown>; writer: string }>('visuals', (e) => {
    if (!group || e.group !== group || e.writer === writer || !(e.version > version)) return;
    version = e.version;
    // Not over a change of this page's still to be sent or on its way: it is the later one, and the server will have
    // it.
    const mine = { ...sending, ...outgoing, routes: { ...((sending.routes as object) ?? {}), ...((outgoing.routes as object) ?? {}) } };
    const settings = { ...e.settings };
    for (const k of Object.keys(mine)) if (k !== 'routes') delete settings[k as keyof typeof settings];
    const routes = Object.keys(mine.routes).length ? (mine.routes as Record<string, number>) : undefined;
    if (routes && settings.routes && typeof settings.routes === 'object') {
      settings.routes = { ...(settings.routes as Record<string, number>), ...routes };
    }
    applyShared(settings);
  });
  const stopWatch = $effect.root(() => {
    $effect(() => {
      const g = groupNow();
      if (g !== group) void joined(g);
    });
  });
  return () => {
    stopEvents();
    stopWatch();
    clearTimeout(pending);
    outgoing = {};
    sending = {};
    group = null;
    version = 0;
  };
}

const status = (e: unknown) => (e instanceof ApiError ? e.status : undefined);

/** Joined `g` (or left a group, with null: what shows stays, as kept on this device already). */
async function joined(g: string | null) {
  group = g;
  version = 0;
  outgoing = {};
  if (!g) return;
  try {
    const got = await getGroupVisuals(g);
    if (group !== g) return;
    version = got.version;
    applyShared(got.settings);
  } catch (e) {
    if (status(e) !== 404 || group !== g) return;
    // A group with none yet takes this device's; where another got there first, its are taken.
    try {
      const made = await putGroupVisuals(g, { base: 0, settings: sharedNow(), writer });
      if (group === g) version = Math.max(version, made.version);
    } catch (e2) {
      if (status(e2) !== 409 || group !== g) return;
      const got = await getGroupVisuals(g).catch(() => null);
      if (got && group === g) {
        version = got.version;
        applyShared(got.settings);
      }
    }
  }
}

/** The owner changed shared settings (`changes`: the settings changed; the routes only those changed): sent to the
 *  group, if the page is in one. */
export function share(changes: Shared) {
  if (!group) return;
  const routes = changes.routes as Record<string, number> | undefined;
  outgoing = { ...outgoing, ...changes, ...(routes ? { routes: { ...((outgoing.routes as Record<string, number>) ?? {}), ...routes } } : {}) };
  clearTimeout(pending);
  pending = setTimeout(flush, 150);
}

async function flush() {
  const g = group;
  const settings = outgoing;
  outgoing = {};
  if (!g || !Object.keys(settings).length) return;
  sending = settings;
  try {
    const done = await putGroupVisuals(g, { base: version, settings, writer });
    if (group === g) version = Math.max(version, done.version);
  } catch {
    // Not sent (the server not reached): the next change sends its own; what shows here stays.
  } finally {
    if (sending === settings) sending = {};
  }
}
