// The server's own settings that the environment seeded until 0.9.8 (the owner, 2026-10-26: what someone running
// Cratarr needs, in Settings, in our patterns): the proxies whose forwarded address is believed (Authentication), the
// analysis here and Beat This! (Libraries), the address players reach playing in step on (Devices). Settings wins once
// saved; `from_env` says which the environment still sets. One read for the three parts; each change saved at once.
// (GET/PUT /api/settings/server-runtime, administrators'.)
import { can } from './capabilities.svelte';

export type ServerRuntime = {
  proxies: string[];
  /** Addresses that count as home besides the local ones (0.10, fencing: other users reach someone's shared
   *  devices from home only); a server before it gives none, and the row stays away. */
  home_networks?: string[];
  /** The guest page (0.10): open on the home network or not, and whose player its guests' tracks are asked of (a
   *  user's, or one player's); a server before it gives none, and its rows stay away. */
  guest?: { on: boolean; host: { user: string } | { player: string } | null };
  analysis: 'keep' | 'all' | 'off';
  beat_this: boolean;
  snapcast_host: string | null;
  from_env?: Partial<Record<'proxies' | 'analysis' | 'beat_this' | 'snapcast_host', boolean>>;
  /** What needs a restart to take effect, when the server says so. */
  restart?: string[];
};

export const runtime = $state({ value: null as ServerRuntime | null, known: false, error: null as string | null });

const URL = '/api/settings/server-runtime';
async function call(init?: RequestInit): Promise<ServerRuntime> {
  const r = await fetch(URL, init);
  if (!r.ok) {
    let why = '';
    try {
      why = (await r.json()).error ?? '';
    } catch {
      /* no words */
    }
    throw Object.assign(new Error(why || `Cratarr said ${r.status}.`), { status: r.status });
  }
  return r.json();
}

let asked = false;
/** Read once (a server before 0.9.8 has none: the rows stay away). */
export function loadRuntime() {
  if (asked) return;
  // Not asked where the server's list says it has none (0.11, lib/capabilities: server_runtime); the rows stay away.
  if (can('server_runtime') === false) return void (runtime.known = false);
  asked = true;
  call().then(
    (v) => ((runtime.value = v), (runtime.known = true)),
    (e: Error & { status?: number }) => {
      runtime.known = e.status !== 404 && e.status !== 405;
      runtime.error = runtime.known ? e.message : null;
    },
  );
}

/** One change saved: the answer is the whole view again; a refusal in one line, left in `error`. */
export async function saveRuntime(change: Partial<Omit<ServerRuntime, 'from_env' | 'restart'>>): Promise<boolean> {
  runtime.error = null;
  try {
    runtime.value = await call({ method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(change) });
    return true;
  } catch (e) {
    runtime.error = e instanceof Error ? e.message : String(e);
    return false;
  }
}
