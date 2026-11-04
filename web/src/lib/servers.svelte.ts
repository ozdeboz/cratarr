// Modes per server (decision 0098): the Jellyfin and Subsonic servers this device is signed in to, as the app keeps
// them (its keystore; never Cratarr's server), added and removed in Settings › This device › Servers. Only in the apps:
// a browser page holds one origin's sign-in. Their requests go out from the app (`server-fetch`), so a page over https
// reaches a server on plain http at home and no server has to allow the page's origin. The code for them
// (lib/servers/*) is loaded only when one is signed in to or its mode opens, not with every page.

import type { AxiosInstance } from 'axios';
import { appInfo, inApp, onApp, toApp } from './app.svelte';
import type { JellyfinClient } from './servers/jellyfin';
import type { MusicServer, ServerKind, ServerSignIn } from './servers/server';

export type { ServerKind, ServerSignIn };

/** Kept here, as the app last told them; null until it has. */
export const servers = $state({ list: null as ServerSignIn[] | null });

onApp('servers', (m) => {
  servers.list = Array.isArray(m.servers) ? (m.servers as ServerSignIn[]) : [];
});
export const askServers = () => toApp({ type: 'servers', op: 'list' });
/** Signed in to, or changed (its scrobbling switch, a sign-in again): kept by the app, which tells the list back. */
export const keepServer = (s: ServerSignIn) => toApp({ type: 'servers', op: 'keep', server: { ...s } });
export const forgetServer = (id: string) => toApp({ type: 'servers', op: 'forget', id });

/** Its kind as the page names it. */
export const KIND_NAMES: Record<ServerKind, string> = { jellyfin: 'Jellyfin', subsonic: 'Subsonic' };

/** A new server's id on this device. */
export const newServerId = () => crypto.randomUUID().replace(/-/g, '').slice(0, 12);

// The app's requests, by their id, until it answers.
const waiting = new Map<string, { ok: (m: Record<string, unknown>) => void; fail: (e: Error) => void }>();
let asked = 0;
onApp('server-fetched', (m) => {
  const w = waiting.get(String(m.id));
  if (!w) return;
  waiting.delete(String(m.id));
  if (typeof m.error === 'string') w.fail(new TypeError(m.error));
  else w.ok(m);
});

function viaApp(url: string, method: string, headers: Record<string, string>, body: string | null): Promise<Record<string, unknown>> {
  const id = `f${++asked}`;
  return new Promise((ok, fail) => {
    waiting.set(id, { ok, fail });
    toApp({ type: 'server-fetch', id, url, method, headers, body });
  });
}

/** `fetch`, through the app (subsonic-api's). */
export const appFetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const headers = Object.fromEntries(new Headers(init?.headers).entries());
  const body = typeof init?.body === 'string' ? init.body : init?.body == null ? null : String(init.body);
  const m = await viaApp(url, init?.method ?? 'GET', headers, body);
  return new Response(String(m.body ?? ''), { status: Number(m.status), headers: (m.headers as Record<string, string>) ?? {} });
}) as typeof fetch;

/** axios, through the app (the Jellyfin SDK's). */
async function appAxios(): Promise<AxiosInstance> {
  const { default: axios } = await import('axios');
  return axios.create({
    adapter: async (config) => {
      const url = axios.getUri(config);
      const headers = Object.fromEntries(Object.entries(config.headers?.toJSON?.() ?? {}).map(([k, v]) => [k, String(v)]));
      const body = config.data == null ? null : typeof config.data === 'string' ? config.data : JSON.stringify(config.data);
      const m = await viaApp(url, (config.method ?? 'get').toUpperCase(), headers, body);
      const status = Number(m.status);
      const text = String(m.body ?? '');
      let data: unknown = text;
      try {
        data = text ? JSON.parse(text) : null;
      } catch {
        // Not JSON: as it came.
      }
      const response = { data, status, statusText: String(status), headers: (m.headers as Record<string, string>) ?? {}, config, request: null };
      if (status >= 400) throw Object.assign(new Error(`status ${status}`), { response, config, isAxiosError: true });
      return response;
    },
  });
}

/** How Cratarr names itself to a Jellyfin: this player's name, and an id for it per server. */
const jellyfinClient = (id: string): JellyfinClient => ({ version: appInfo.version ?? '0', device: appInfo.player ?? 'Cratarr', deviceId: `cratarr-${id}` });

/** Signs in to a server, which the app then keeps. Quick Connect is Jellyfin's (lib/servers/jellyfin). */
export async function signIn(kind: ServerKind, url: string, user: string, secret: string, apiKey = false): Promise<ServerSignIn> {
  const id = newServerId();
  const at = url.trim().replace(/\/+$/, '');
  if (kind === 'subsonic') {
    const { signInSubsonic } = await import('./servers/subsonic');
    return signInSubsonic(id, at, user.trim(), secret, apiKey, appFetch);
  }
  const { signInJellyfin } = await import('./servers/jellyfin');
  return signInJellyfin(id, at, user.trim(), secret, jellyfinClient(id), await appAxios());
}

export async function startQuickConnect(url: string) {
  const id = newServerId();
  const { quickConnect } = await import('./servers/jellyfin');
  return quickConnect(id, url.trim().replace(/\/+$/, ''), jellyfinClient(id), await appAxios());
}

// One reader per server, made the first time its mode asks.
const open = new Map<string, Promise<MusicServer>>();

/** The server's reader, its requests through the app. */
export function serverOf(s: ServerSignIn): Promise<MusicServer> {
  const key = `${s.id}:${s.secret}`;
  let got = open.get(key);
  if (!got) {
    got =
      s.kind === 'subsonic'
        ? import('./servers/subsonic').then(({ subsonicServer }) => subsonicServer(s, appFetch))
        : Promise.all([import('./servers/jellyfin'), appAxios()]).then(([{ jellyfinServer }, ax]) => jellyfinServer(s, jellyfinClient(s.id), ax));
    open.set(key, got);
  }
  return got;
}

/** The apps keep servers; a browser does not. */
export const keepsServers = () => inApp;
