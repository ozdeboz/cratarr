// Play On to this page (decision 0033: a page is a Play On target only while it is open; 0031 amended, the owner
// 2026-10-16): while a page in a browser plays here (its output is this browser), it is one of Cratarr's players,
// under this browser's name (Settings → This device), as the app is: Cratarr's commands play on its own player
// (localPlayer's playOnCommand) and it reports what plays, on every change and every few seconds. Closed, it is gone
// from the list (offline). A browser lets sound start only after the page was touched once: a play before that shows
// "Tap to play". Behind a locked screen a paused page is frozen after a minute or so, and shows offline until it is
// looked at again; playing, it stays. No sync play in a browser: it joins no group.
import { getPlayers } from './api';
import { me } from './me.svelte';
import { inApp } from './app.svelte';
import { canPlayHere, local, pageVolume, playOnCommand } from './localPlayer.svelte';
import { playOn, drivenName } from './playOn.svelte';
import { prefs } from './prefs.svelte';
import { asksSignIn, refused, refusedFor, toSignIn } from './reach.svelte';
import { setOwnSleep, sleep } from './sleep.svelte';

/** Every so often a report, even with nothing changed: the position, and that the page is still there. */
const HEARTBEAT_MS = 5_000;
/** After a connection that failed: again after this long, doubled each time, up to a minute. */
const RETRY_MS = 5_000;

export const target = $state({ connected: false, tapToPlay: false });

/** This browser's name as a player: the one set, else its browser and system ("Chrome on Android"). */
export function defaultBrowserName(): string {
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  const browser = /Edg\//.test(ua) ? 'Edge' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : 'Browser';
  const os = /Android/.test(ua) ? 'Android' : /iPhone|iPad/.test(ua) ? 'iOS' : /Windows/.test(ua) ? 'Windows' : /Mac OS X/.test(ua) ? 'Mac' : /Linux/.test(ua) ? 'Linux' : '';
  return os ? `${browser} on ${os}` : browser;
}

/** The first of `base`, "`base` 2", … that none of `players` has. */
function freeName(base: string, players: { name: string }[]): string {
  const taken = new Set(players.map((p) => p.name));
  let name = base;
  for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
  return name;
}

/** Whether a player name is another user's (the server refuses it with 409 and says so). The connect is asked again
 *  only for its answer's head: one that would connect is let go at once. */
async function anotherUsers(n: string): Promise<boolean> {
  const ctl = new AbortController();
  try {
    const res = await fetch(`/api/players/${encodeURIComponent(n)}/connect?kind=browser&page=${PAGE}`, { signal: ctl.signal });
    if (res.status !== 409) return false;
    const body = await res.json().catch(() => ({}));
    // `taken` since stage 3; the words before it.
    return body?.taken === true || (typeof body?.error === 'string' && body.error.includes("another user's player"));
  } catch {
    return false;
  } finally {
    ctl.abort();
  }
}

/** In place of another user's player's name: this user's own added, else the next number. */
function ownName(n: string): string {
  return me.name && !n.endsWith(` (${me.name})`) ? `${n} (${me.name})` : freeName(n, [{ name: n }]);
}

/** A name no other player has (a second Chrome on Android is "Chrome on Android 2"), kept from then on. */
function nameToUse(): string {
  if (prefs.browserName) return prefs.browserName;
  prefs.browserName = freeName(defaultBrowserName(), playOn.players);
  return prefs.browserName;
}

/** An entry id (decision 0033) for a track this page added itself; `crypto.randomUUID` needs a secure page. */
function uuid(): string {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/** This page's own id (one per tab, kept over a reload): Cratarr lets no other page take its name while it holds it. */
const PAGE = (() => {
  try {
    const kept = sessionStorage.getItem('cratarr-page');
    if (kept) return kept;
    const id = uuid();
    sessionStorage.setItem('cratarr-page', id);
    return id;
  } catch {
    return uuid();
  }
})();

// Its own log while it is open (decision 0045, item 2), numbered, the newest 500 kept: what became of its connection.
// Each time it connects, the lines Cratarr has not had yet go to it. A new log each time the page loads.
const LOG = `${PAGE}-${Date.now().toString(36)}`;
type Line = { seq: number; at: number; level: 'info' | 'warn'; area: 'playon'; what: string; fields: Record<string, unknown> };
const lines: Line[] = [];
let seq = 0;
function note(level: Line['level'], what: string, fields: Record<string, unknown> = {}) {
  lines.push({ seq: ++seq, at: Date.now(), level, area: 'playon', what, fields });
  if (lines.length > 500) lines.shift();
}

/** The lines Cratarr has not had yet, to it. */
async function sendLog(player: string) {
  const base = `/api/players/${encodeURIComponent(player)}/log`;
  try {
    const held = await fetch(`${base}?log=${encodeURIComponent(LOG)}`);
    if (!held.ok) return;
    const last = ((await held.json()) as { last?: number }).last ?? 0;
    const out = lines.filter((l) => l.seq > last);
    if (out.length) await fetch(base, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ log: LOG, lines: out }) });
  } catch {
    /* sent with the next connection */
  }
}

// The link as this page sees it, in its reports.
let connectedSince = 0;
let failed = 0;
let lastError: string | null = null;
let waitUntil = 0;
let reportsThrough = true;

let source: EventSource | null = null;
let name: string | null = null;
let version: number | undefined;
let sentQueue = '';
let retry = RETRY_MS;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
let reportTimer: ReturnType<typeof setTimeout> | undefined;

/** What plays, to Cratarr; with the queue when it changed since the last report (or `withQueue`). */
async function report(withQueue = false) {
  // Not while the proxy refuses this page (lib/reach): each would be one more knock on a ban.
  if (!name || !target.connected || refusedFor() > 0) return;
  // Tracks this page added itself get an entry, as the app gives them.
  for (const t of local.queue) if (!t.entry) t.entry = uuid();
  const key = local.queue.map((t) => t.entry).join(',');
  const queue = withQueue || key !== sentQueue;
  const t = local.index >= 0 ? local.queue[local.index] : undefined;
  const s = sleep();
  const a = local.autoplay;
  const body = {
    index: local.index,
    ...(t?.entry ? { entry: t.entry } : {}),
    ...(version !== undefined ? { version } : {}),
    repeat: local.repeat,
    sleep: s ? { at_ms: s.atMs, end_of_track: s.endOfTrack } : null,
    radio: local.radio,
    radio_by: local.radioBy,
    radio_count: local.radioCount,
    skip_favourites: local.skipFavourites,
    autoplay: a ? { mode: a.mode, albums: a.albums, restart: a.restart, seed: a.seed, within: a.within } : null,
    position_ms: Math.round(local.position * 1000),
    duration_ms: local.duration ? Math.round(local.duration * 1000) : null,
    playing: local.index >= 0 && !local.paused,
    volume: pageVolume.level,
    ducked: pageVolume.ducked,
    link: {
      failed,
      address: location.origin,
      ...(connectedSince ? { connected_since: connectedSince } : {}),
      ...(lastError ? { last_error: lastError } : {}),
      ...(waitUntil > Date.now() ? { wait_until: waitUntil, wait_why: refusedFor() > 0 ? 'refused' : 'after a failure' } : {}),
    },
    ...(queue
      ? {
          queue: local.queue.map((q) => ({
            entry: q.entry,
            source: q.source,
            id: q.id,
            name: q.name,
            artist: q.artist,
            album: q.album,
            album_id: q.album_id,
            ...(q.lane ? { lane: q.lane } : {}),
          })),
        }
      : {}),
  };
  try {
    const res = await fetch(`/api/players/${encodeURIComponent(name)}/state`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (res.ok && queue) sentQueue = key;
    if (asksSignIn(res)) toSignIn();
    else if (res.status === 403 || res.status === 401) refused();
    reported(res.ok, `HTTP ${res.status}`);
  } catch (e) {
    /* the next report tries again */
    reported(false, e instanceof Error ? e.message : String(e));
  }
}

/** Whether reports get through: written down as it changes, not every report. */
function reported(ok: boolean, error: string) {
  if (ok === reportsThrough) return;
  reportsThrough = ok;
  if (ok) note('info', 'reports through');
  else note('warn', 'reports failing', { error });
}

function reportSoon(withQueue = false) {
  clearTimeout(reportTimer);
  reportTimer = setTimeout(() => report(withQueue), 250);
}

function connect() {
  disconnect();
  name = nameToUse();
  const es = new EventSource(`/api/players/${encodeURIComponent(name)}/connect?kind=browser&page=${PAGE}`);
  source = es;
  const player = name;
  es.onopen = () => {
    note('info', 'connected', { address: location.origin, ...(failed ? { failed_before: failed } : {}) });
    target.connected = true;
    connectedSince = Date.now();
    failed = 0;
    lastError = null;
    waitUntil = 0;
    retry = RETRY_MS;
    report(true);
    sendLog(player);
  };
  es.addEventListener('command', (e) => {
    let m: Record<string, unknown>;
    try {
      m = JSON.parse((e as MessageEvent).data);
    } catch {
      return;
    }
    if (typeof m.version === 'number') version = m.version;
    if (m.command === 'sleep') {
      const atMs = typeof m.at_ms === 'number' ? m.at_ms : null;
      setOwnSleep(atMs === null && m.end_of_track !== true ? null : { atMs, endOfTrack: m.end_of_track === true });
    } else if (playOnCommand(m)) {
      // A play the browser may hold back: until the page has been touched once, it asks for that.
      const touched = (navigator as Navigator & { userActivation?: { hasBeenActive: boolean } }).userActivation?.hasBeenActive;
      if (touched === false) target.tapToPlay = true;
      else setTimeout(() => (target.tapToPlay = local.paused && local.index >= 0), 1500);
    }
    reportSoon(m.command === 'play');
  });
  es.onerror = () => {
    // Refused or gone for good (an error answer ends an EventSource): again later, never in a rush (a proxy bans
    // what keeps knocking).
    if (es.readyState === EventSource.CLOSED) {
      // An EventSource says nothing of why: ended after it was open, or refused (or out of reach) before.
      if (target.connected) note('info', 'connection ended');
      else {
        failed++;
        lastError = navigator.onLine ? 'refused or out of reach' : 'offline';
        if (failed === 1) note('warn', 'connection failed', { error: lastError });
      }
      target.connected = false;
      connectedSince = 0;
      clearTimeout(retryTimer);
      const later = () => {
        const wait = Math.max(retry, refusedFor());
        waitUntil = Date.now() + wait;
        if (refusedFor() > 0) note('warn', 'waiting', { until: waitUntil, why: 'refused' });
        retryTimer = setTimeout(() => want() && connect(), wait);
        retry = Math.min(retry * 2, 60_000);
      };
      // Its name held by another page (another tab, another browser) or the app: the next free one ("… 2"), at once.
      // Held by another user's player (several users, decision 0051; another user's players are not listed, so the
      // refusal is asked for its reason): one's own name added ("Chrome on Android (anna)"), at once.
      getPlayers().then(async (list) => {
        const players = list?.players ?? [];
        if (source === es && name && players.some((p) => p.name === name && p.online)) prefs.browserName = freeName(name, players);
        else if (source === es && name && (await anotherUsers(name))) prefs.browserName = ownName(name);
        else later();
      }, later);
    }
  };
}

function disconnect() {
  source?.close();
  source = null;
  target.connected = false;
  sentQueue = '';
  clearTimeout(retryTimer);
}

/** A page that plays here, in a browser (in the app the app is the player; driving another player, it is a remote). */
const want = () => !inApp && canPlayHere && prefs.output === 'local' && drivenName() === null;

if (typeof window !== 'undefined' && !inApp) {
  $effect.root(() => {
    // Connected while it plays here, under its name (a new name: again under that one).
    $effect(() => {
      const on = want();
      const n = prefs.browserName;
      if (!on) return disconnect();
      // Connected under this name already (the first connect saves the name it chose).
      if (source && n === name) return;
      connect();
    });
    // Every change reported at once.
    $effect(() => {
      void [local.index, local.paused, local.queue, local.repeat, local.radio, local.autoplay, local.skipFavourites, pageVolume.level, pageVolume.ducked, sleep()];
      if (target.connected) reportSoon();
    });
    // Sound has started: no need to ask for the tap any more.
    $effect(() => {
      if (!local.paused) target.tapToPlay = false;
    });
  });
  setInterval(() => target.connected && report(), HEARTBEAT_MS);
}
