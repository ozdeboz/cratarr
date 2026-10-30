import { startStopOf, type StartStop } from './startStop';
import type { AudioOutput } from './api';
import { rampBegins, rampEnds, rewindNow } from './ramping.svelte';

// The Android app (decision 0031): on the server's own pages inside it, `window.CratarrApp` carries JSON messages both
// ways (android/README.md). There the music plays in the app itself; the page is its face (localPlayer.svelte.ts).

type Port = { postMessage(message: string): void; onmessage: ((event: { data: string }) => void) | null };
type Message = { type: string; [key: string]: unknown };

const port = typeof window === 'undefined' ? null : ((window as unknown as { CratarrApp?: Port }).CratarrApp ?? null);

/** Whether this page runs in the Android app. */
export const inApp = !!port;

/** What the app told about itself: its version, and this device's settings (the Settings page's "This device"). */
/** Set by lib/playOn: tells an app with no target yet the page's last choice (decision 0062). */
let seedTarget: (() => void) | null = null;
export const onSeedTarget = (f: () => void) => (seedTarget = f);

export const appInfo = $state({
  version: null as string | null,
  /** The app turns a Start and stop ramp round at a press mid-way (0.9.15); before, its button waited for the end. */
  turnsRound: false,
  /** This player's name, as Home Assistant and the other players see it. */
  player: null as string | null,
  /** Network (after Finamp's): the public address; the local network address, used while preferred and answering; the
   *  address in use. `addresses`: the app knows two (an app from before knew one, its `server`). */
  publicAddress: null as string | null,
  localAddress: null as string | null,
  /** The local network address refused this app's sign-in a moment ago (0.9.7): the app goes by the public one. */
  localRefused: false,
  preferLocal: false,
  inUse: null as string | null,
  addresses: false,
  /** An address typed in Network where Cratarr does not answer (not kept); null after any other answer. */
  refused: null as string | null,
  /** Kept awake and listening for Play On while idle. */
  stayReady: false,
  /** Android lets the app run in the background (left out of battery optimisation). */
  batteryAllowed: true,
  /** Downloads as the files are, or as Cratarr's smaller copies (decision 0033, step 4). */
  downloadQuality: 'original' as 'original' | 'smaller',
  /** Downloads over mobile data too, not only over Wi-Fi. */
  downloadOverMobile: false,
  /** Kept on this device (decision 0038): the storage budget in GB, 0 for no limit; syncing only while charging. */
  syncBudgetGb: 20,
  syncOnlyCharging: false,
  /** Levelling (decision 0035): the app's player plays every track about as loud as the next. */
  levelling: true,
  levelUnknown: 'typical' as 'typical' | 'untouched',
  startStop: startStopOf({}),
  /** Where this device can sound now (decision 0033, amended 2026-10-19: the Play On sheet chooses), and the one
   *  chosen; null: Android's own choice (Automatic), as again whenever an output comes or goes. */
  outputs: [] as AppOutput[],
  output: null as string | null,
  /** Automatic in the user's order (the owner: "Automatic follows it", with a switch back to Android's): the first
   *  connected one in `known`, every output seen, in that order. */
  ordered: false,
  known: [] as AppOutput[],
  /** Local mode (decision 0052): which mode plays, Cratarr's or the device's own music; `local`: a local folder is on
   *  and Android lets the app read them (the switcher shows); Android's permission for the device's music. */
  mode: 'cratarr' as 'cratarr' | 'device',
  /** Where this device's controls point (decision 0062), as the app keeps it: `self`, `player:<name>` or `server`;
   *  null from an app before it, where the page keeps its own. */
  target: null as string | null,
  local: false,
  deviceMusic: 'unasked' as 'unasked' | 'denied' | 'granted',
});

/** One of the device's audio outputs: `id` "type:name" (or the type alone, for one with no name). */
export type AppOutput = { id: string } & AudioOutput;

/** A track kept on the device: as the page described it when it was downloaded, and how far the download is. */
export type DownloadedTrack = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  download: { id: string; state: 'done' | 'downloading' | 'waiting' | 'failed'; bytes: number; percent: number };
  /** Downloaded once by the owner (false: there only for what is synced); absent from an app before 0.8. */
  manual?: boolean;
};

/** The owner's downloads on this device, as the app last told them (`known` once it has). */
export const downloads = $state({ tracks: [] as DownloadedTrack[], bytes: 0, known: false });

const handlers = new Map<string, (m: Message) => void>();

/** Calls `handle` with each message of `type` from the app. */
export function onApp(type: string, handle: (m: Message) => void) {
  handlers.set(type, handle);
}

export function toApp(message: Message) {
  port?.postMessage(JSON.stringify(message));
}

/** "Test both connections": whether each address answered (null: not set), once the app has tried. */
export const networkTest = $state({ testing: false, local: null as boolean | null, public: null as boolean | null, signedOut: false, done: false });

/** Tries both addresses (the app answers with a `network-test`). */
export function testNetwork() {
  networkTest.testing = true;
  networkTest.done = false;
  toApp({ type: 'network-test' });
}

/** Changes this device's settings; the app answers with all of them. */
export const setDevice = (change: {
  player?: string;
  /** The public address (`server` too, for an app from before there were two). */
  public?: string;
  server?: string;
  local?: string;
  preferLocal?: boolean;
  stayReady?: boolean;
  downloadQuality?: string;
  downloadOverMobile?: boolean;
  syncBudgetGb?: number;
  syncOnlyCharging?: boolean;
  levelling?: boolean;
  /** A track not measured yet: turned down as a typical one, or left as it is (lib/prefs levelUnknown). */
  levelUnknown?: 'typical' | 'untouched';
  /** How play and pause start and stop (lib/prefs StartStop), the app's own player's. */
  startStop?: StartStop;
  /** This device's audio output by its id; null: Automatic. */
  output?: string | null;
  /** Automatic in the user's order, or Android's; the order (ids of `known`); one not connected forgotten. */
  ordered?: boolean;
  order?: string[];
  forget?: string;
}) =>
  toApp({ type: 'device-set', ...change });
/** Keeps these tracks on the device, in the quality the settings say. */
export const download = (tracks: { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null }[]) =>
  toApp({ type: 'download', tracks: tracks.map(({ source, id, name, artist, album, album_id }) => ({ source, id, name, artist, album, album_id })) });
/** A readied export saved where the app's user picks, under `name` (Download… as files, 0.10): the app streams it
 *  from the export's address (lib/api exportDownloadUrl), one file as itself, several as one zip. */
export const saveExport = (token: string, name: string) => toApp({ type: 'export-save', token, name });
/** A track as Download… gives it to the app, to this device. */
export type Downloadable = Parameters<typeof download>[0][number];
/** Asks the app for its downloads; it tells them again on every change. */
export const askDownloads = () => toApp({ type: 'downloads' });
/** Takes downloads off the device: these (by their download id), or all. */
export const removeDownloads = (ids: string[] | 'all') => toApp(ids === 'all' ? { type: 'download-remove', all: true } : { type: 'download-remove', ids });
/** Opens Android's dialog that lets the app run in the background. */
export const allowBackground = () => toApp({ type: 'battery' });
/** Android's page of the app, its battery use one tap away: where running in the background is taken back. */
export const backgroundSettings = () => toApp({ type: 'battery', op: 'settings' });
/** Asks the app for its settings again. */
export const askDevice = () => toApp({ type: 'device' });

if (port) {
  port.onmessage = (event) => {
    let m: Message;
    try {
      m = JSON.parse(event.data);
    } catch {
      return;
    }
    if (m.type === 'app') {
      appInfo.version = typeof m.version === 'string' ? m.version : null;
      appInfo.player = typeof m.player === 'string' ? m.player : null;
      appInfo.addresses = typeof m.public === 'string';
      appInfo.publicAddress = typeof m.public === 'string' ? m.public : typeof m.server === 'string' ? m.server : null;
      appInfo.localAddress = typeof m.local === 'string' && m.local ? m.local : null;
      appInfo.preferLocal = m.preferLocal === true;
      appInfo.localRefused = m.localRefused === true;
      appInfo.inUse = typeof m.inUse === 'string' && m.inUse ? m.inUse : appInfo.publicAddress;
      appInfo.refused = typeof m.refused === 'string' && m.refused ? m.refused : null;
      appInfo.stayReady = m.stayReady === true;
      appInfo.batteryAllowed = m.batteryAllowed !== false;
      appInfo.downloadQuality = m.downloadQuality === 'smaller' ? 'smaller' : 'original';
      appInfo.downloadOverMobile = m.downloadOverMobile === true;
      appInfo.syncBudgetGb = typeof m.syncBudgetGb === 'number' ? m.syncBudgetGb : 20;
      appInfo.syncOnlyCharging = m.syncOnlyCharging === true;
      appInfo.levelling = m.levelling !== false;
      appInfo.levelUnknown = m.levelUnknown === 'untouched' ? 'untouched' : 'typical';
      {
        // What an app from before the Tape page does not keep (its motions, the tape's mechanics) stays as set here,
        // never snapping back to the defaults (the owner's tablet, 2026-10-27).
        const told = (m.startStop ?? {}) as Partial<StartStop>;
        const was = appInfo.startStop;
        appInfo.startStop = startStopOf({
          ...told,
          tape: told.tape ?? was.tape,
          motions: told.motions ?? was.motions,
          edges: told.edges ?? was.edges,
        });
      }
      appInfo.outputs = Array.isArray(m.outputs) ? (m.outputs as AppOutput[]).filter((o) => typeof o?.id === 'string') : [];
      appInfo.output = typeof m.output === 'string' && appInfo.outputs.some((o) => o.id === m.output) ? m.output : null;
      appInfo.ordered = m.ordered === true;
      appInfo.known = Array.isArray(m.known) ? (m.known as AppOutput[]).filter((o) => typeof o?.id === 'string') : [];
      appInfo.mode = m.mode === 'device' ? 'device' : 'cratarr';
      // A Start and stop ramp turned round at a press mid-way (0.9.15): its play button live through it.
      appInfo.turnsRound = m.turnsRound === true;
      if (typeof m.target === 'string') appInfo.target = m.target;
      // An app that keeps the target but has none yet: the page's last choice, once (decision 0062).
      else if (m.target === null && appInfo.target === null) seedTarget?.();
      // Local mode on (decision 0052): `localMode` (0.9.7); before it `local`, which the address of the same name was lost
      // under (the owner's phone, 2026-10-26: Local network address "Not set").
      appInfo.local = m.localMode === true || m.local === true;
      appInfo.deviceMusic = m.deviceMusic === 'granted' || m.deviceMusic === 'denied' ? m.deviceMusic : 'unasked';
    }
    // Where the controls point changed, in the app (decision 0062): the page shows it.
    if (m.type === 'target' && typeof m.target === 'string') appInfo.target = m.target;
    // The app's player ramping (Start and stop): the play/pause button shows it; never a tape rewind's own speeding
    // up and slowing down (the owner, 2026-10-27: the button fills and empties for play and pause only).
    if (m.type === 'ramp' && !rewindNow.on) {
      // `effect: false`: the app's own ramp that stops no music (a skip's wind-down): no grey, no drained button.
      if ((m.dir === 'out' || m.dir === 'in') && typeof m.ms === 'number') rampBegins(m.dir, m.ms, typeof m.full === 'number' ? m.full : m.ms, m.effect !== false);
      else rampEnds();
    }
    if (m.type === 'network-test') {
      networkTest.local = typeof m.local === 'boolean' ? m.local : null;
      networkTest.public = typeof m.public === 'boolean' ? m.public : null;
      // `publicRefused`: an app from before the sign-in page (its stored login refused).
      networkTest.signedOut = m.signedOut === true || m.publicRefused === true;
      networkTest.testing = false;
      networkTest.done = true;
    }
    if (m.type === 'downloads') {
      downloads.tracks = Array.isArray(m.tracks) ? (m.tracks as DownloadedTrack[]) : [];
      downloads.bytes = typeof m.bytes === 'number' ? m.bytes : 0;
      downloads.known = true;
    }
    handlers.get(m.type)?.(m);
  };
  toApp({ type: 'hello' });
}
