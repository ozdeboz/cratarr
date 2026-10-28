// Home in local mode (decision 0052; docs/plans.md "Home offline, and Home in local mode"): the device's own library,
// which Cratarr never sees, so a smaller Home of its own, worked out here from what the device knows: the newest by when
// they came onto it, and what was played here (the app's `device-plays`: its player plays with the page closed too).
// The sections that need the analysis, the listening history or biographies, which only the server has, are left out;
// Settings › Home shows them greyed, "Needs your server", the account's choice for them kept (the owner, 2026-10-24).
import type { HomeBack, ListedAlbum } from './api';
import { inApp, onApp, toApp } from './app.svelte';
import { asLocal, device, DEVICE, deviceAlbums, type DeviceTrack } from './deviceMusic.svelte';
import { playQueue } from './localPlayer.svelte';
import type { HomePart } from './prefs.svelte';

/** Home's sections local mode has. */
export const LOCAL_PARTS: HomePart[] = ['newest', 'back'];
/** How many albums a section shows. */
const SHOWN = 20;

/** A play here that counted (90%), latest first: the track as device.tracks has it, when (epoch ms), how often. */
export type DevicePlay = { id: string; at: number; count: number };
export const devicePlays = $state({ list: [] as DevicePlay[], known: false });
if (inApp)
  onApp('device-plays', (m) => {
    devicePlays.list = Array.isArray(m.plays) ? (m.plays as DevicePlay[]).filter((p) => typeof p?.id === 'string') : [];
    devicePlays.known = true;
  });
/** Asks the app for its plays here again. */
export const askDevicePlays = () => inApp && toApp({ type: 'device-plays' });

/** The device's newest albums, by when they came onto it. */
export const newestHere = (): ListedAlbum[] =>
  deviceAlbums()
    .sort((a, b) => (b.added ?? 0) - (a.added ?? 0))
    .slice(0, SHOWN);

/** The albums played here last, as Jump back in lists them. */
export function backHere(): HomeBack[] {
  const byId = new Map((device.tracks ?? []).map((t) => [t.id, t]));
  const albums = new Map(deviceAlbums().map((a) => [a.id, a]));
  const out: HomeBack[] = [];
  for (const p of devicePlays.list) {
    const id = byId.get(p.id)?.album_id;
    const a = id ? albums.get(id) : undefined;
    if (!a || out.some((x) => x.id === a.id)) continue;
    out.push({ kind: 'album', at: Math.floor(p.at / 1000), source: DEVICE, id: a.id, title: a.title, by: a.artist });
    if (out.length === SHOWN) break;
  }
  return out;
}

const byPlace = (a: DeviceTrack, b: DeviceTrack) => (a.disc ?? 1) - (b.disc ?? 1) || (a.track ?? 0) - (b.track ?? 0);
/** Plays the device's albums one after the other, each in its order, from the first track of `from`'s. */
export function playHereAlbums(ids: string[], from: string, name: string) {
  const tracks = ids.flatMap((id) => (device.tracks ?? []).filter((t) => t.album_id === id).sort(byPlace));
  if (!tracks.length) return;
  playQueue(tracks.map(asLocal), Math.max(0, tracks.findIndex((t) => t.album_id === from)), name);
}
