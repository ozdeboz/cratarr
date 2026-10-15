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
});

const handlers = new Map<string, (m: Message) => void>();

/** Calls `handle` with each message of `type` from the app. */
export function onApp(type: string, handle: (m: Message) => void) {
  handlers.set(type, handle);
}

export function toApp(message: Message) {
  port?.postMessage(JSON.stringify(message));
}

/** Changes this device's settings; the app answers with all of them. */
export const setDevice = (change: { player?: string; stayReady?: boolean }) => toApp({ type: 'device-set', ...change });
/** Opens Android's dialog that lets the app run in the background. */
export const allowBackground = () => toApp({ type: 'battery' });
/** Opens the app's prompt for the server's address. */
export const changeAddress = () => toApp({ type: 'address' });
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
    }
    handlers.get(m.type)?.(m);
  };
  toApp({ type: 'hello' });
}
