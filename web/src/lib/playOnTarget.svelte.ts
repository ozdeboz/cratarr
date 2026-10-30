// Play On to this page (decision 0033: a page is a Play On target only while it is open; 0031 amended, the owner
// 2026-10-16): while a page in a browser is open, it is one of Cratarr's players, under this browser's name (Settings →
// This browser), as the app is: Cratarr's commands play on its own player (localPlayer's playOnCommand) and it reports
// what plays, on every change and every few seconds. While its screen drives another player it plays nothing itself;
// music sent to it turns the screen here, and the other plays on (amended 2026-10-23). In a group it follows, as a Kodi
// does (lib/groupListen): its own music stops, the group's sound plays, and its screen shows the group's. Closed, it is
// gone from the list (offline). A browser lets sound start only after the page was touched once: a play before that shows
// "Tap to play". Behind a locked screen a paused page is frozen after a minute or so, and shows offline until it is
// looked at again; playing, it stays. No sync play in a browser: it joins no group.
import { lastSeq, logLines, note, sendSoonWith } from './deviceLog';
import { describing } from './described.svelte';
import { desktop } from './desktop';
import { getPlayers } from './api';
import { me } from './me.svelte';
import { inApp } from './app.svelte';
import { canPlayHere, local, pageVolume, playOnCommand, rewindNow, stop } from './localPlayer.svelte';
import { tapeDir } from './ramping.svelte';
import { followGroup, leaveGroup, listening } from './groupListen.svelte';
import { playOn, drivenName, hereAgain } from './playOn.svelte';
import { followTo } from './output.svelte';
import { prefs } from './prefs.svelte';
import { appInfo } from './app.svelte';
import { asksSignIn, refused, refusedFor, toSignIn } from './reach.svelte';
import { setOwnSleep, sleep } from './sleep.svelte';
import { oneInFlight } from './oneInFlight';

/** Every so often a report, even with nothing changed: the position, and that the page is still there. */
const HEARTBEAT_MS = 5_000;
/** After a connection that failed: again after this long, doubled each time, up to a minute. */
const RETRY_MS = 5_000;
/** Back from a sleep or a lost network (0.10; the owner, 2026-10-30: the ThinkPad came back a minute after its
 *  Wi-Fi did, and a cast to it failed meanwhile): this long it tries again often, at most this far apart. */
const WAKE_MS = 30_000;
const WAKE_RETRY_MS = 1_500;

export const target = $state({ connected: false, tapToPlay: false });

/** This browser's name as a player: the one set, else its browser and system ("Chrome on Android"); in the desktop app,
 *  the computer's name. */
export function defaultBrowserName(): string {
  if (desktop?.name) return desktop.name;
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  const browser = /Edg\//.test(ua) ? 'Edge' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : 'Browser';
  const os = /Android/.test(ua) ? 'Android' : /iPhone|iPad/.test(ua) ? 'iOS' : /Windows/.test(ua) ? 'Windows' : /Mac OS X/.test(ua) ? 'Mac' : /Linux/.test(ua) ? 'Linux' : '';
  return os ? `${browser} on ${os}` : browser;
}

/** A name without the numbers a free name was given ("Edge on Windows 3 2 2" is "Edge on Windows"). */
const unnumbered = (name: string) => name.replace(/( \d+)+$/, '') || name;

/** The first of `base`, "`base` 2", … that none of `players` has; counted from the name without its numbers, so a
 *  name renumbered again never grows ("… 3 2 2", the owner's player list, 2026-10-24). */
function freeName(base: string, players: { name: string }[]): string {
  const taken = new Set(players.map((p) => p.name));
  const root = unnumbered(base);
  let name = base;
  for (let n = 2; taken.has(name); n++) name = `${root} ${n}`;
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

/** This tab's own name while another tab of this browser holds the browser's ("ThinkBook 2"), for as long as the tab
 *  is open and the browser's name stays what it was (`base`); kept in the tab only, so tabs never number each other's
 *  names. */
type TabName = { base: string; name: string };
const TAB_KEY = 'cratarr-tab-name';
const tabName = $state<{ value: TabName | null }>({
  value: (() => {
    try {
      const kept = JSON.parse(sessionStorage.getItem(TAB_KEY) ?? 'null');
      return kept && typeof kept.base === 'string' && typeof kept.name === 'string' ? kept : null;
    } catch {
      return null;
    }
  })(),
});
function setTabName(name: string) {
  const t = { base: prefs.browserName, name };
  try {
    sessionStorage.setItem(TAB_KEY, JSON.stringify(t));
  } catch {
    /* kept while the page is loaded */
  }
  tabName.value = t;
}

/** A name no other player has (a second Chrome on Android is "Chrome on Android 2"), kept from then on; in the desktop
 *  app the computer's name as it is. */
function nameToUse(): string {
  // A stored name grown by a page from before freeName counted from the root ("Safari on iOS 2 2 2 2 2 2 2", the owner's
  // iPhone, 2026-10-26): its numbers off, then the first free one again.
  if (/( \d+){2,}$/.test(prefs.browserName)) prefs.browserName = freeName(unnumbered(prefs.browserName), playOn.players);
  // The desktop app is the computer's own player: a player of this account by its name is taken over, not numbered past
  // (the owner, 2026-10-29: the app replaces the ThinkPad's browser player, its name kept for Home Assistant's rules).
  if (!prefs.browserName) prefs.browserName = desktop?.name ? desktop.name : freeName(defaultBrowserName(), playOn.players);
  const t = tabName.value;
  return t && t.base === prefs.browserName ? t.name : prefs.browserName;
}

/** An entry id (decision 0033) for a track this page added itself; `crypto.randomUUID` needs a secure page. */
function uuid(): string {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/** This page's own id (one per tab, kept over a reload): Cratarr lets no other page take its name while it holds it.
 *  In the desktop app, one window, kept over a restart too, so a restarted app takes its own name back at once (0.10,
 *  2026-10-30). */
const PAGE = (() => {
  try {
    const store = desktop ? localStorage : sessionStorage;
    const kept = store.getItem('cratarr-page');
    if (kept) return kept;
    const id = uuid();
    store.setItem('cratarr-page', id);
    return id;
  } catch {
    return uuid();
  }
})();

// Its own log while it is open (lib/deviceLog). Each time it connects, the lines Cratarr has not had yet go to it. A new
// log each time the page loads.
const LOG = `${PAGE}-${Date.now().toString(36)}`;

/** The last line Cratarr has had. */
let logged = 0;
/** Every so often while connected, the lines Cratarr has not had yet (what a group's sound did, as it happens). */
const LOG_MS = 30_000;

/** The lines Cratarr has not had yet, to it. */
async function sendLogNow(player: string) {
  const base = `/api/players/${encodeURIComponent(player)}/log`;
  try {
    const held = await fetch(`${base}?log=${encodeURIComponent(LOG)}`);
    if (!held.ok) return;
    const last = ((await held.json()) as { last?: number }).last ?? 0;
    const out = logLines.filter((l) => l.seq > last);
    if (out.length && (await fetch(base, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ log: LOG, lines: out }) })).ok)
      logged = out[out.length - 1].seq;
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
/** Until when a wake's quick tries go on (0: none). */
let wakeUntil = 0;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
let reportTimer: ReturnType<typeof setTimeout> | undefined;

/** The lines Cratarr has not had yet, one sending at a time (lib/oneInFlight). */
const sendLog = oneInFlight(sendLogNow, (_was: string, now: string) => now);

/** What plays, to Cratarr; with the queue when it changed since the last report (or `withQueue`). */
async function sendReport(withQueue = false) {
  // Not while the proxy refuses this page (lib/reach): each would be one more knock on a ban.
  if (!name || !target.connected || refusedFor() > 0) return;
  // Following a group, it keeps no queue of its own: what it plays is the group's sound.
  const idle = elsewhere() || listening.leader !== null;
  // Tracks this page added itself get an entry, as the app gives them.
  if (!idle) for (const t of local.queue) if (!t.entry) t.entry = uuid();
  const own = idle ? [] : local.queue;
  const key = idle ? 'elsewhere' : own.map((t) => t.entry).join(',');
  const queue = withQueue || key !== sentQueue;
  const index = idle ? -1 : local.index;
  const t = index >= 0 ? own[index] : undefined;
  const s = sleep();
  const a = local.autoplay;
  const body = {
    index,
    ...(t?.entry ? { entry: t.entry } : {}),
    ...(version !== undefined ? { version } : {}),
    repeat: local.repeat,
    sleep: s ? { at_ms: s.atMs, end_of_track: s.endOfTrack } : null,
    radio: local.radio,
    radio_by: local.radioBy,
    radio_count: local.radioCount,
    skip_favourites: local.skipFavourites,
    autoplay: a ? { mode: a.mode, albums: a.albums, restart: a.restart, seed: a.seed, within: a.within } : null,
    position_ms: idle ? 0 : Math.round(local.position * 1000),
    duration_ms: !idle && local.duration ? Math.round(local.duration * 1000) : null,
    playing: listening.leader !== null ? listening.playing : !idle && local.index >= 0 && !local.paused,
    // Winding (decision 0074): its position goes back and forth, and none of it is listening (the server counts none).
    rewinding: rewindNow.on,
    volume: pageVolume.level,
    ducked: pageVolume.ducked,
    // Level the volume, as another device driving this one sets and shows it (the owner, 2026-10-27).
    levelling: prefs.levelling,
    direction: tapeDir.want,
    level_unknown: prefs.levelUnknown,
    level_db: local.levelDb,
    starting: local.starting,
    link: {
      failed,
      address: location.origin,
      ...(connectedSince ? { connected_since: connectedSince } : {}),
      ...(lastError ? { last_error: lastError } : {}),
      ...(waitUntil > Date.now() ? { wait_until: waitUntil, wait_why: refusedFor() > 0 ? 'refused' : 'after a failure' } : {}),
    },
    ...(queue
      ? {
          // Which tracks, not what they are, where the server describes them (lib/described); a server before it is
          // still told the details.
          queue: own.map((q) => ({
            entry: q.entry,
            source: q.source,
            id: q.id,
            ...(describing.server ? {} : { name: q.name, artist: q.artist, album: q.album, album_id: q.album_id }),
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

/** A report, one on its way at a time (lib/oneInFlight: those that piled up on a slow server held a hold's stream back
 *  from it); asked meanwhile, one more after it with what plays then, with the queue if any of the asks wanted it. */
const reporting = oneInFlight(sendReport, (was: boolean, now: boolean) => was || now);
const report = (withQueue = false) => reporting(withQueue);

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
  playOn.self = name;
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
    // What starts or stops the sound, in the device's log, as it came (0.10, 2026-10-30: a restarted ThinkPad played on
    // by itself, and only the server's log could say why).
    if (m.command === 'play' || m.command === 'resume' || m.command === 'pause' || m.command === 'stop' || m.command === 'toggle')
      note('info', 'command', { command: m.command, ...(m.command === 'play' ? { paused: m.paused === true, index: m.index, position_ms: m.position_ms } : {}) });
    // Sync play: in a group led by another, its sound here and its screen on the group (its own music stops); alone
    // again, quiet.
    if (m.command === 'group') {
      const leader = typeof m.leader === 'string' && m.leader !== player ? m.leader : null;
      if (!leader) leaveGroup();
      else {
        if (elsewhere()) hereAgain();
        else stop();
        followGroup(leader, player, note).then((sounds) => (target.tapToPlay = !sounds));
      }
      return reportSoon(true);
    }
    // Its screen drives another player: music sent here turns it here (the other plays on); the rest is for a player
    // that plays nothing.
    if (elsewhere()) {
      if (m.command !== 'play' && m.command !== 'add') return reportSoon();
      hereAgain();
    }
    if (m.command === 'sleep') {
      const atMs = typeof m.at_ms === 'number' ? m.at_ms : null;
      setOwnSleep(atMs === null && m.end_of_track !== true ? null : { atMs, endOfTrack: m.end_of_track === true });
    } else if (m.command === 'stop' && typeof m.moved_to === 'string' && m.moved_to !== player) {
      // The music moved to another player (a handover there): this one stops, and its screen follows the music
      // (0.10; the owner, 2026-10-30: the ThinkPad stayed on its own empty player when the tablet took it over).
      playOnCommand(m);
      followTo(m.moved_to);
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
        retry = Date.now() < wakeUntil ? WAKE_RETRY_MS : Math.min(retry * 2, 60_000);
      };
      // Its name held by another page (another tab, another browser) or the app: this tab takes the next free one
      // after the browser's ("… 2"), at once, for itself only. Held by another user's player (several users, decision
      // 0051; another user's players are not listed, so the refusal is asked for its reason): one's own name added
      // ("Chrome on Android (anna)"), at once.
      getPlayers().then(async (list) => {
        const players = list?.players ?? [];
        if (source === es && name && players.some((p) => p.name === name && p.online))
          setTabName(freeName(prefs.browserName, players.filter((p) => p.online)));
        else if (source === es && name && (await anotherUsers(name))) prefs.browserName = ownName(name);
        else later();
      }, later);
    }
  };
}

/** Awake again (the computer, the network, the page in sight): a connection from before may be dead without having
 *  said so (a suspended computer's), so a fresh one at once, the wait between tries short for a while. Not while the
 *  proxy refuses this page (lib/reach). */
function again(why: string) {
  if (!want() || refusedFor() > 0) return;
  wakeUntil = Date.now() + WAKE_MS;
  retry = WAKE_RETRY_MS;
  clearTimeout(retryTimer);
  note('info', 'reconnecting', { why });
  source?.close();
  source = null;
  target.connected = false;
  connect();
}

function disconnect() {
  leaveGroup();
  source?.close();
  source = null;
  target.connected = false;
  sentQueue = '';
  clearTimeout(retryTimer);
}

/** A page in a browser that can play (in the app the app is the player): a player while it is open, whatever its
 *  screen drives. */
const want = () => !inApp && canPlayHere;
/** Its screen drives another player: its own plays nothing meanwhile (`local` holds the other's). */
const elsewhere = () => drivenName() !== null || prefs.output !== 'local';

if (typeof window !== 'undefined' && !inApp) {
  $effect.root(() => {
    // Connected while it plays here, under its name (a new name: again under that one).
    $effect(() => {
      const on = want();
      const n = nameToUse();
      if (!on) return disconnect();
      // Connected under this name already (the first connect saves the name it chose).
      if (source && n === name) return;
      connect();
    });
    // Every change reported at once: each of the report's fields but the position, which the heartbeat carries (the
    // architecture audit, 2026-10-29: winding, a ramp's start, Radio's by and count and the group's leader waited for it).
    $effect(() => {
      void [local.index, local.paused, local.queue, local.repeat, local.radio, local.radioBy, local.radioCount, local.autoplay, local.skipFavourites, local.starting, rewindNow.on, pageVolume.level, pageVolume.ducked, sleep(), prefs.output, listening.playing, listening.leader, tapeDir.want, prefs.levelling, prefs.levelUnknown, local.levelDb];
      if (target.connected) reportSoon();
    });
    // Sound has started: no need to ask for the tap any more.
    $effect(() => {
      if (!local.paused) target.tapToPlay = false;
    });
  });
  setInterval(() => target.connected && report(), HEARTBEAT_MS);
  // Awake again: the network back, the page shown again after a while hidden (a sleep, a locked screen), or the
  // desktop app's word that the computer woke (desktop/main.js: powerMonitor's resume and unlock-screen).
  addEventListener('online', () => again('online'));
  let hiddenAt = 0;
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) hiddenAt = Date.now();
    else if (hiddenAt && Date.now() - hiddenAt > WAKE_MS) again('shown');
  });
  desktop?.onWake?.(() => again('woke'));
  setInterval(() => target.connected && name && lastSeq() > logged && sendLog(name), LOG_MS);
  // A warning (a track that will not play): within moments.
  let soonTimer: ReturnType<typeof setTimeout> | undefined;
  sendSoonWith(() => {
    clearTimeout(soonTimer);
    soonTimer = setTimeout(() => target.connected && name && sendLog(name), 2000);
  });
}

/** This device's name as Play On knows it: the app's player, this browser's (the owner, 2026-10-25: Settings' "This
 *  device" says it, "Salon tablet"); "This device" before it is known. */
export const deviceName = () => (inApp ? appInfo.player : prefs.browserName || defaultBrowserName()) || 'This device';
