// The Android app (decision 0031): on the server's own pages inside it, `window.CratarrApp` carries JSON messages both
// ways (android/README.md). There the music plays in the app itself; the page is its face (localPlayer.svelte.ts).

type Port = { postMessage(message: string): void; onmessage: ((event: { data: string }) => void) | null };
type Message = { type: string; [key: string]: unknown };

const port = typeof window === 'undefined' ? null : ((window as unknown as { CratarrApp?: Port }).CratarrApp ?? null);

/** Whether this page runs in the Android app. */
export const inApp = !!port;

/** What the app told about itself: its version, and this device's settings (the Settings page's "This device"). */
export const appInfo = $state({
  version: null as string | null,
  /** This player's name, as Home Assistant and the other players see it. */
  player: null as string | null,
  /** Network (after Finamp's): the public address; the local network address, used while preferred and answering; the
   *  address in use. `addresses`: the app knows two (an app from before knew one, its `server`). */
  publicAddress: null as string | null,
  localAddress: null as string | null,
  preferLocal: false,
  inUse: null as string | null,
  addresses: false,
  /** An address typed in Network where Cratarr does not answer (not kept); null after any other answer. */
  refused: null as string | null,
  /** The sign-in the public address asks for away from home: its user name, and whether a password is kept. */
  signInUser: '',
  signInSet: false,
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
});

/** A track kept on the device: as the page described it when it was downloaded, and how far the download is. */
export type DownloadedTrack = {
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  download: { id: string; state: 'done' | 'downloading' | 'waiting' | 'failed'; bytes: number; percent: number };
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
export const networkTest = $state({ testing: false, local: null as boolean | null, public: null as boolean | null, publicRefused: false, done: false });

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
  /** The sign-in: a user name (empty clears it), a password only when one is typed. */
  signInUser?: string;
  signInPassword?: string;
  stayReady?: boolean;
  downloadQuality?: string;
  downloadOverMobile?: boolean;
  syncBudgetGb?: number;
  syncOnlyCharging?: boolean;
  levelling?: boolean;
}) =>
  toApp({ type: 'device-set', ...change });
/** Keeps these tracks on the device, in the quality the settings say. */
export const download = (tracks: { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null }[]) =>
  toApp({ type: 'download', tracks: tracks.map(({ source, id, name, artist, album, album_id }) => ({ source, id, name, artist, album, album_id })) });
/** Asks the app for its downloads; it tells them again on every change. */
export const askDownloads = () => toApp({ type: 'downloads' });
/** Takes downloads off the device: these (by their download id), or all. */
export const removeDownloads = (ids: string[] | 'all') => toApp(ids === 'all' ? { type: 'download-remove', all: true } : { type: 'download-remove', ids });
/** Opens Android's dialog that lets the app run in the background. */
export const allowBackground = () => toApp({ type: 'battery' });
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
      appInfo.inUse = typeof m.inUse === 'string' && m.inUse ? m.inUse : appInfo.publicAddress;
      appInfo.refused = typeof m.refused === 'string' && m.refused ? m.refused : null;
      appInfo.signInUser = typeof m.signInUser === 'string' ? m.signInUser : '';
      appInfo.signInSet = m.signInSet === true;
      appInfo.stayReady = m.stayReady === true;
      appInfo.batteryAllowed = m.batteryAllowed !== false;
      appInfo.downloadQuality = m.downloadQuality === 'smaller' ? 'smaller' : 'original';
      appInfo.downloadOverMobile = m.downloadOverMobile === true;
      appInfo.syncBudgetGb = typeof m.syncBudgetGb === 'number' ? m.syncBudgetGb : 20;
      appInfo.syncOnlyCharging = m.syncOnlyCharging === true;
      appInfo.levelling = m.levelling !== false;
    }
    if (m.type === 'network-test') {
      networkTest.local = typeof m.local === 'boolean' ? m.local : null;
      networkTest.public = typeof m.public === 'boolean' ? m.public : null;
      networkTest.publicRefused = m.publicRefused === true;
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
