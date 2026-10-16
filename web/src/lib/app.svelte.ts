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
  /** The Cratarr server's address the app shows. */
  server: null as string | null,
  /** Kept awake and listening for Play On while idle. */
  stayReady: false,
  /** Android lets the app run in the background (left out of battery optimisation). */
  batteryAllowed: true,
  /** Downloads as the files are, or as Cratarr's smaller copies (decision 0033, step 4). */
  downloadQuality: 'original' as 'original' | 'smaller',
  /** Downloads over mobile data too, not only over Wi-Fi. */
  downloadOverMobile: false,
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

/** Changes this device's settings; the app answers with all of them. */
export const setDevice = (change: { player?: string; server?: string; stayReady?: boolean; downloadQuality?: string; downloadOverMobile?: boolean }) =>
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
      appInfo.server = typeof m.server === 'string' ? m.server : null;
      appInfo.stayReady = m.stayReady === true;
      appInfo.batteryAllowed = m.batteryAllowed !== false;
      appInfo.downloadQuality = m.downloadQuality === 'smaller' ? 'smaller' : 'original';
      appInfo.downloadOverMobile = m.downloadOverMobile === true;
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
