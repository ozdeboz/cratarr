// The visuals shared by a sync play group (decision 0046, amended; the server keeps each group's settings:
// /api/groups/{group}/visuals). Everything that shapes the picture is shared, so a slider moved on one device moves on
// all of them: the look (its colours among it) and its pins, intensity, flash, speed, the grain and haze, the routes and
// each band's times. What each device can afford (Quality, frame rate) and its brightness (a bright room, a dark one)
// stay its own.
// A device joining a group adopts the group's settings (a group with none yet takes the joiner's); one leaving keeps
// what it was showing. What the owner changes goes to the server as it is made, a drag's many changes once every 150 ms
// and only the settings changed (the server merges them, the routes route by route, so two people moving different
// sliders do not undo each other), and comes back to every device on the live channel's `visuals` event, with all of
// the group's settings; a device knows its own changes by `writer` and takes none older than the last it took.
// A track's preset and the music's moments are not sent: every device works out the same ones. So the look's settings
// not pinned are the group's only on the track they were sent on (`lookFor`: the dice's, a Reset's, a recall's); on
// another track each device has that track's own, as every other device does.
// The saved looks (snapshots, on the server for every device) hold the same settings.

import { ApiError, getGroupVisuals, putGroupVisuals } from '../api';
import { on } from '../events.svelte';
import { head, playOn } from '../playOn.svelte';
import { PRESET_KEYS, type Preset } from './flame';
import { SCALE_KEYS } from './routes';
import { keptFrom, saveVisualSettings, visualSettings, type VisualSettings } from './settings.svelte';

/** The settings shared, and saved in a snapshot. */
export const SHARED_KEYS = [
  // The patch beyond today's chain (decision 0056), whole.
  'extra',
  ...PRESET_KEYS,
  'pins',
  'wild',
  'intensity',
  'flash',
  'speed',
  'smoothing',
  'haze',
  'fluid',
  'tunnel',
  'loose',
  'effects',
  'flameLayer',
  'dive',
  ...SCALE_KEYS,
  'routes',
  'lighting',
  'evolution',
  'continuity',
] as const satisfies readonly (keyof VisualSettings)[];
export type Shared = Partial<Record<(typeof SHARED_KEYS)[number] | 'lookFor', unknown>>;
/** What each device keeps its own in a group (what it can afford, its room's brightness), but a snapshot keeps too:
 *  recalled, they come back on the device that recalls it, and are never sent to the group. */
export const DEVICE_KEYS = ['quality', 'fps', 'brightness'] as const satisfies readonly (keyof VisualSettings)[];
/** A snapshot's form, kept with it so a later one can be read: 1, the shared settings (the first ones, from 63); 2,
 *  every slider (2026-10-18); 3, 65's (Quality, Haze, the Colours as the look's: what they replaced is not read, by the
 *  owner's choice). */
export const SNAPSHOT_V = 3;

/** Every slider as it is now, for a snapshot. */
export function snapshotNow(): Record<string, unknown> {
  return { ...sharedNow(), ...Object.fromEntries(DEVICE_KEYS.map((k) => [k, $state.snapshot(visualSettings[k])])), v: SNAPSHOT_V };
}

/** A snapshot's own settings of this device into it (only those it has: the first snapshots have none); null for
 *  Resolution or FPS is this device's default. */
export function applyDevice(settings: Record<string, unknown>) {
  const given = Object.fromEntries(DEVICE_KEYS.filter((k) => k in settings).map((k) => [k, settings[k]]));
  if (!Object.keys(given).length) return;
  const clean = keptFrom({ ...$state.snapshot(visualSettings), ...given });
  for (const k of Object.keys(given)) (visualSettings as Record<string, unknown>)[k] = clean[k as keyof VisualSettings];
  saveVisualSettings();
}

/** This page, as its changes come back from the server. */
const writer = (globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`).slice(0, 64);

/** The shared settings as they are now. */
export function sharedNow(): Shared {
  return Object.fromEntries(SHARED_KEYS.map((k) => [k, $state.snapshot(visualSettings[k])]));
}

/** Shared settings into this device's (each brought into its range): of the look's, those pinned, and those not pinned
 *  where they were sent for the track showing. */
export function applyShared(settings: Record<string, unknown>) {
  const given = { ...settings };
  const pins = Array.isArray(given.pins) ? (given.pins as string[]) : visualSettings.pins;
  if (given.lookFor !== track) for (const k of PRESET_KEYS) if (!pins.includes(k)) delete given[k];
  const clean = keptFrom({ ...$state.snapshot(visualSettings), ...given, pins });
  for (const k of SHARED_KEYS) if (k in given) (visualSettings as Record<string, unknown>)[k] = clean[k];
  saveVisualSettings();
}

// The group followed (its leader's name), the last version taken, its settings as last heard (and as this page has
// changed them since), the changes waiting to be sent and the ones on their way, and the track the page shows.
let group: string | null = null;
let version = 0;
let known: Record<string, unknown> = {};
let outgoing: Shared = {};
let sending: Shared = {};
let pending: ReturnType<typeof setTimeout> | undefined;
let track: string | null = null;

/** Settings merged into others, the routes route by route. */
const merged = (into: Record<string, unknown>, more: Record<string, unknown>) => ({
  ...into,
  ...more,
  ...(more.routes && typeof more.routes === 'object' ? { routes: { ...((into.routes as object) ?? {}), ...(more.routes as object) } } : {}),
});

/** The track the visuals show now (`source/id`). */
export function showing(key: string | null) {
  track = key;
}

/** The look the group set for track `key` (the dice's, a Reset's, a recall's), where it set one: taken in place of the
 *  track's own when it starts (a device whose track changed after the group's look was sent). */
export function groupLook(key: string | null): Partial<Preset> | null {
  if (!group || !key || known.lookFor !== key) return null;
  return Object.fromEntries(PRESET_KEYS.filter((k) => k in known).map((k) => [k, known[k]])) as Partial<Preset>;
}

/** Whose visuals this page shares (the server's key): its player's group's (the leader's name) where the player is in
 *  one, else the player's own name, so a page driving a player shares them with that player's own page (the owner,
 *  2026-10-18: "My phone is most definitely controlling my salon tablet … So they are in a group to some extent").
 *  The player is the app's own, or the one the page plays on. */
const groupNow = () => {
  const name = head();
  return name ? playOn.players.find((p) => p.name === name)?.group || name : null;
};

/** Follows the group the page's player is in while the visuals show; returns the function that stops it. */
export function followGroup(): () => void {
  const stopEvents = on<{ group: string; version: number; settings: Record<string, unknown>; writer: string }>('visuals', (e) => {
    if (!group || e.group !== group || e.writer === writer || !(e.version > version)) return;
    version = e.version;
    known = merged(known, e.settings);
    // Not over a change of this page's still to be sent or on its way: it is the later one, and the server will have
    // it.
    const mine = merged(sending, outgoing);
    const settings = { ...e.settings };
    for (const k of Object.keys(mine)) if (k !== 'routes' && k !== 'lookFor') delete settings[k];
    if (mine.routes && settings.routes && typeof settings.routes === 'object') settings.routes = { ...(settings.routes as object), ...(mine.routes as object) };
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
    known = {};
    group = null;
    version = 0;
  };
}

const status = (e: unknown) => (e instanceof ApiError ? e.status : undefined);

/** Joined `g` (or left a group, with null: what shows stays, as kept on this device already). */
async function joined(g: string | null) {
  group = g;
  version = 0;
  known = {};
  outgoing = {};
  if (!g) return;
  const take = (got: { version: number; settings: Record<string, unknown> }) => {
    version = got.version;
    known = { ...got.settings };
    applyShared(got.settings);
  };
  try {
    const got = await getGroupVisuals(g);
    if (group === g) take(got);
  } catch (e) {
    if (status(e) !== 404 || group !== g) return;
    // A group with none yet takes this device's; where another got there first, its are taken.
    const mine = { ...sharedNow(), lookFor: track };
    try {
      const made = await putGroupVisuals(g, { base: 0, settings: mine, writer });
      if (group !== g) return;
      version = Math.max(version, made.version);
      known = merged(known, mine);
    } catch (e2) {
      if (status(e2) !== 409 || group !== g) return;
      const got = await getGroupVisuals(g).catch(() => null);
      if (got && group === g) take(got);
    }
  }
}

/** The owner changed shared settings (`changes`: the settings changed; the routes only those changed): sent to the
 *  group, if the page is in one. Look settings go with the track they are for. */
export function share(changes: Shared) {
  if (!group) return;
  const sent: Shared = PRESET_KEYS.some((k) => k in changes) ? { ...changes, lookFor: track } : changes;
  outgoing = merged(outgoing, sent);
  known = merged(known, sent);
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

/** The visuals as Cratarr made them on 2026-10-18 (preview 62), kept as data so they can always be brought back, however
 *  the defaults change later: each track's own look (nothing pinned), and everything else as it was then (Resolution
 *  and FPS each device's own default). Listed first
 *  among the saved looks; it cannot be renamed or deleted. A recall puts back its routes whole: any route it does not
 *  have is turned off. */
export const STANDARD = {
  id: 'standard',
  name: 'Standard',
  created: 0,
  settings: {
    v: 3,
    // 62's Points, Steps and Resolution (19, 3 and the screen's default) are Quality's 60 %; its Floor 0 and Depth 50 %
    // Haze's 0; its Colours (the preset's own) the look's, nothing pinned.
    quality: 0.6,
    fps: null,
    brightness: 1,
    pins: [],
    intensity: 1,
    flash: 1,
    speed: 1,
    smoothing: 0.5,
    haze: 0,
    fluid: 0,
    tunnel: 0,
    loose: 0,
    dive: 0,
    lighting: [0.2, 0.18, 0.15, 0.13, 0.1, 0.08],
    evolution: [0.25, 0.25, 0.25, 0.25, 0.25, 0.25],
    continuity: [0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
    routes: {
      'loudness|-|size': 1,
      'loudness|-|saturation': 1,
      'loudness|-|rotation': 0.2,
      'beat|-|brightness': 0.2,
      'beat|-|size': 0.08,
      ...Object.fromEntries(
        [0, 1, 2, 3, 4, 5].flatMap((b) => [
          [`level|${b}|shape`, 1],
          [`level|${b}|rotation`, 0.6],
          [`level|${b}|hue`, 0.08],
          [`hits|${b}|lights`, 1],
        ]),
      ),
    },
  },
} as const;
