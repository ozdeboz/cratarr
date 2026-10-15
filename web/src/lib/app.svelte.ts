// The Android app (decision 0031): on the server's own pages inside it, `window.CratarrApp` carries JSON messages both
// ways (android/README.md). There the music plays in the app itself; the page is its face (localPlayer.svelte.ts).

type Port = { postMessage(message: string): void; onmessage: ((event: { data: string }) => void) | null };
type Message = { type: string; [key: string]: unknown };

const port = typeof window === 'undefined' ? null : ((window as unknown as { CratarrApp?: Port }).CratarrApp ?? null);

/** Whether this page runs in the Android app. */
export const inApp = !!port;

/** What the app told about itself: its version and this player's name. */
export const appInfo = $state({ version: null as string | null, player: null as string | null });

const handlers = new Map<string, (m: Message) => void>();

/** Calls `handle` with each message of `type` from the app. */
export function onApp(type: string, handle: (m: Message) => void) {
  handlers.set(type, handle);
}

export function toApp(message: Message) {
  port?.postMessage(JSON.stringify(message));
}

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
    }
    handlers.get(m.type)?.(m);
  };
  toApp({ type: 'hello' });
}
