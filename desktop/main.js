// Cratarr's desktop app (decision 0078), Linux first: Cratarr's own pages in a window of their own. Thin, as the
// Android app began (decision 0031): the window shows Cratarr's page on the server's address, and the page plays as it
// does in a browser. What the shell adds to an installed page:
// - it carries its own copy of the page (decision 0085, step 1; page.js), answered on the server's address in place of
//   the server's, so it opens at once and with the server out of reach, a blank window never;
// - it can start at login and keeps running when its window is closed (in the tray), so the computer stays one of
//   Cratarr's players;
// - it plays without a first click, and a hidden window keeps its timers (its reports, the next track);
// - it works with any server, the local http address too: that origin is treated as secure, so the page's service
//   worker, and with it Downloads, work there as on https.
'use strict';

const { app, BrowserWindow, Menu, Tray, ipcMain, nativeImage, net, powerMonitor, screen, session, shell } = require('electron');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { Readable, Writable } = require('node:stream');
const { buildOf, ownPage } = require('./page');

/** The shell's own settings: the server, the window's place. */
const SETTINGS = path.join(app.getPath('userData'), 'settings.json');
const ICON = path.join(__dirname, 'icons', 'icon-512.png');
const BACKGROUND = '#0f1115';
/** Started at login: no window shown, but the page loads, so the computer is a player at once. */
const HIDDEN = process.argv.includes('--hidden');
/** Linux has no login items in Electron: an XDG autostart entry starts it. */
const AUTOSTART = path.join(app.getPath('appData'), 'autostart', 'cratarr.desktop');

function readSettings() {
  try {
    return JSON.parse(fs.readFileSync(SETTINGS, 'utf8'));
  } catch {
    return {};
  }
}

function writeSettings(change) {
  const next = { ...readSettings(), ...change };
  fs.mkdirSync(path.dirname(SETTINGS), { recursive: true });
  fs.writeFileSync(SETTINGS, JSON.stringify(next, null, 2));
}

/** An address as typed ("192.0.2.10:8081", "cratarr.example.org", "https://…/home") as an origin, or null. */
function originOf(address) {
  const typed = String(address ?? '').trim();
  if (!typed) return null;
  // Without a scheme: http for an IP address or a name with a port (a server on the home network), else https.
  const bare = /^[a-z][a-z0-9+.-]*:\/\//i.test(typed) ? typed : (/^[\d.]+(:\d+)?(\/|$)|:\d+(\/|$)/.test(typed) ? 'http://' : 'https://') + typed;
  try {
    const url = new URL(bare);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.origin : null;
  } catch {
    return null;
  }
}

const settings = readSettings();
/** The server's origin, or null before one is chosen. */
const origin = originOf(settings.server);
/** Cratarr's page as its health check last told it (`web`: its release and build), kept for the next start. */
let told = buildOf(settings.serverPage);
/** The app's own copy of the page (copy-page.js put it there). */
const page = ownPage(app.isPackaged ? path.join(process.resourcesPath, 'page') : path.join(__dirname, 'page'), () => told);

// Before the app is ready: what the page may do here that a browser tab may not.
app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
if (origin?.startsWith('http:')) app.commandLine.appendSwitch('unsafely-treat-insecure-origin-as-secure', origin);

/** One shell at a time: started again, it shows its window. */
const first = app.requestSingleInstanceLock();
if (!first) app.quit();

let win = null;
let tray = null;
let quitting = false;

/** How long a quit may take to close its window before the app ends anyway (ms). */
const QUIT_GRACE = 3_000;
let quitTimer = null;
/** Quitting, however asked (the tray, the menu, a signal, the system), ends the app, never hangs it (0.10.1: while apt
 *  replaced the app's files, the running copy's Quit did nothing and it ignored SIGTERM until killed). The window's
 *  close is let through (`quitting`), its place kept now rather than in a close that may never come; a window whose
 *  page does not answer (its renderer gone or stuck, its files replaced under it) cannot hold the quit: after
 *  QUIT_GRACE the app exits. Nothing here is loaded at quit time. */
function beginQuit() {
  quitting = true;
  if (quitTimer) return;
  quitTimer = setTimeout(() => app.exit(0), QUIT_GRACE);
  try {
    if (win && !win.isDestroyed()) writeSettings({ bounds: win.getNormalBounds() });
  } catch {
    /* the place is not worth a hang */
  }
}
function quitNow() {
  beginQuit();
  app.quit();
}

const sameOrigin = (url) => {
  try {
    return !!origin && new URL(url).origin === origin;
  } catch {
    return false;
  }
};

/** A request of the page's, sent on as it came (its body as it comes; the session's cookies to Cratarr alone, none to
 *  another site), the answer streamed back. A redirect is handed back as one, so the page follows it itself and its
 *  address says where it went (the sign-in); `net.fetch` follows it here instead, and its manual mode only refuses it.
 *  An answer the page lets go of before its end (a closed event stream, a seek in a stream) ends the request too. */
function sendOn(req) {
  return new Promise((resolve, reject) => {
    const r = net.request({
      method: req.method,
      url: req.url,
      origin: req.headers.get('origin') ?? undefined,
      credentials: sameOrigin(req.url) ? 'include' : 'omit',
      redirect: 'manual',
      bypassCustomProtocolHandlers: true,
    });
    for (const [k, v] of req.headers) r.setHeader(k, v);
    // A page load said to be one, as the page's browser says it on its own way out (here it would go as `no-cors`):
    // Cratarr sends a page load without a session to the sign-in page, and anything else gets a refusal.
    if (req.method === 'GET' && req.headers.has('upgrade-insecure-requests') && req.headers.get('accept')?.includes('text/html')) {
      r.setHeader('Sec-Fetch-Mode', 'navigate');
      r.setHeader('Sec-Fetch-Dest', 'document');
    }
    r.on('redirect', (status, _method, url) => {
      r.abort();
      resolve(new Response(null, { status, headers: { location: url } }));
    });
    r.on('response', (res) => {
      let ended = false;
      res.on('end', () => (ended = true));
      res.on('close', () => ended || r.abort());
      const headers = new Headers();
      for (const [k, v] of Object.entries(res.headers)) for (const one of [].concat(v)) headers.append(k, one);
      const empty = [101, 204, 205, 304].includes(res.statusCode) || req.method === 'HEAD';
      resolve(new Response(empty ? null : Readable.toWeb(res), { status: res.statusCode, statusText: res.statusMessage, headers }));
    });
    r.on('error', reject);
    if (req.body) req.body.pipeTo(Writable.toWeb(r)).catch(reject);
    else r.end();
  });
}

/** Cratarr's page as its health check tells it, at the start, every ten minutes and on waking, for the choice of page
 *  (page.js); kept as it was while Cratarr is out of reach. */
async function hearPage() {
  if (!origin) return;
  try {
    const answer = await net.fetch(`${origin}/api/health`, { bypassCustomProtocolHandlers: true, signal: AbortSignal.timeout(8_000) });
    const body = await answer.json();
    if (body?.status !== 'ok' || typeof body.playlists !== 'number') return;
    const web = buildOf(body.web);
    if (JSON.stringify(web) === JSON.stringify(told)) return;
    told = web;
    writeSettings({ serverPage: web });
  } catch {
    /* out of reach: the last told stands */
  }
}

/** Another site's link opens in the computer's browser, never in Cratarr's window. */
function openOutside(url) {
  if (/^(https?|mailto):/i.test(url)) shell.openExternal(url);
}

function show() {
  if (!win) return;
  if (win.isMinimized()) win.restore();
  win.show();
  win.focus();
}

function startsAtLogin() {
  if (process.platform !== 'linux') return app.getLoginItemSettings().openAtLogin;
  return fs.existsSync(AUTOSTART);
}

function setStartAtLogin(on) {
  if (process.platform !== 'linux') {
    app.setLoginItemSettings({ openAtLogin: on, args: ['--hidden'] });
  } else if (on) {
    // An AppImage starts through its own file: the running copy is a mount that goes away.
    const exec = process.env.APPIMAGE || process.execPath;
    fs.mkdirSync(path.dirname(AUTOSTART), { recursive: true });
    fs.writeFileSync(
      AUTOSTART,
      ['[Desktop Entry]', 'Type=Application', 'Name=Cratarr', `Exec="${exec}" --hidden`, 'Icon=cratarr', 'X-GNOME-Autostart-enabled=true', ''].join('\n'),
    );
  } else {
    fs.rmSync(AUTOSTART, { force: true });
  }
  menus();
}

/** Back to the server's address: shown again, with the one in use filled in. */
function changeServer() {
  show();
  win?.loadFile(path.join(__dirname, 'setup.html'));
}

/** The tray's menu and the window's (shown with Alt). */
function menus() {
  const login = { label: 'Start at login', type: 'checkbox', checked: startsAtLogin(), click: (item) => setStartAtLogin(item.checked) };
  const server = { label: 'Change server…', click: changeServer };
  // The tray's Quit without a shortcut: a tray's menu has none on Linux (GTK complains of each).
  tray?.setContextMenu(
    Menu.buildFromTemplate([{ label: 'Show Cratarr', click: show }, server, login, { type: 'separator' }, { label: 'Quit Cratarr', click: quitNow }]),
  );
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      { label: 'Cratarr', submenu: [server, login, { type: 'separator' }, { label: 'Quit Cratarr', accelerator: 'CmdOrCtrl+Q', click: quitNow }] },
      {
        label: 'View',
        submenu: [
          { role: 'reload' },
          { role: 'forceReload' },
          { type: 'separator' },
          { role: 'resetZoom' },
          { role: 'zoomIn' },
          { role: 'zoomOut' },
          { type: 'separator' },
          { role: 'togglefullscreen' },
          { role: 'toggleDevTools' },
        ],
      },
    ]),
  );
}

/** The window's last place, where it is still on a screen; else only its size (a screen since unplugged). */
function lastPlace() {
  const b = settings.bounds;
  if (!b) return {};
  const seen = screen.getAllDisplays().some(({ workArea: a }) => b.x < a.x + a.width && b.x + b.width > a.x && b.y < a.y + a.height && b.y + b.height > a.y);
  return seen ? b : { width: b.width, height: b.height };
}

/** What the page may ask of the computer: copying, fullscreen (the map, the visuals), keeping the screen on (the
 *  visuals), a speaker of its choice, storage that is kept (below). Nothing else: no camera, microphone or location. */
const ALLOWED = new Set(['clipboard-sanitized-write', 'fullscreen', 'screen-wake-lock', 'speaker-selection', 'persistent-storage']);

function createWindow() {
  win = new BrowserWindow({
    width: 1280,
    height: 820,
    ...lastPlace(),
    minWidth: 360,
    minHeight: 480,
    show: false,
    title: 'Cratarr',
    icon: ICON,
    backgroundColor: BACKGROUND,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      backgroundThrottling: false,
      spellcheck: false,
      additionalArguments: [`--cratarr-desktop=${app.getVersion()}`, `--cratarr-desktop-name=${os.hostname()}`],
    },
  });
  win.once('ready-to-show', () => {
    if (!HIDDEN) win.show();
  });
  win.on('close', (event) => {
    // Closed, it keeps playing and stays a player, in the tray; Quit (there, in the menu, a signal) ends it.
    if (!quitting && tray) {
      event.preventDefault();
      try {
        writeSettings({ bounds: win.getNormalBounds() });
      } catch {
        /* kept the next time */
      }
      win.hide();
    }
  });
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (sameOrigin(url)) return { action: 'allow', overrideBrowserWindowOptions: { autoHideMenuBar: true, backgroundColor: BACKGROUND, icon: ICON } };
    openOutside(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (event, url) => {
    if (sameOrigin(url) || url.startsWith('file:')) return;
    event.preventDefault();
    openOutside(url);
  });
  // The server out of reach (the NAS restarting, the network down): tried again, sooner at first.
  let retry = 5_000;
  win.webContents.on('did-fail-load', (_event, code, _description, url, mainFrame) => {
    if (!mainFrame || code === -3 || !sameOrigin(url)) return;
    setTimeout(() => win?.loadURL(url), retry);
    retry = Math.min(retry * 2, 60_000);
  });
  win.webContents.on('did-finish-load', () => {
    retry = 5_000;
    // What the page keeps on the computer (its last copies, its downloads, the library's index) is kept for good, not
    // let go of when the disk runs low, as a browser does with a site's.
    if (sameOrigin(win.webContents.getURL())) win.webContents.executeJavaScript('navigator.storage?.persist?.()').catch(() => {});
  });
  win.webContents.on('render-process-gone', (_event, details) => {
    if (details.reason !== 'clean-exit' && !quitting) win?.webContents.reload();
  });
  if (origin) win.loadURL(origin + '/');
  else win.loadFile(path.join(__dirname, 'setup.html'));
}

// The setup page's two questions; only it may ask them.
const fromSetup = (event) => event.senderFrame?.url.startsWith('file:') ?? false;
ipcMain.handle('server', (event) => (fromSetup(event) ? (origin ?? settings.server ?? null) : null));
ipcMain.handle('connect', async (event, address) => {
  if (!fromSetup(event)) return false;
  const chosen = originOf(address);
  if (!chosen) return false;
  try {
    const answer = await fetch(`${chosen}/api/health`, { signal: AbortSignal.timeout(8_000) });
    const body = await answer.json();
    if (body?.status !== 'ok' || typeof body.playlists !== 'number') return false;
  } catch {
    return false;
  }
  writeSettings({ server: chosen });
  // The switches set before start (the origin treated as secure) need a fresh start.
  const args = process.argv.slice(1).filter((a) => a !== '--hidden');
  app.relaunch(process.env.APPIMAGE ? { execPath: process.env.APPIMAGE, args } : { args });
  quitting = true;
  app.exit(0);
  return true;
});

if (first) {
  app.on('second-instance', show);
  // Every quit, the system's too (a log-out, an updater), with the same bound on how long it may take.
  app.on('before-quit', beginQuit);
  // A signal (the package's after-install, a log-out, `kill`): a quit like the tray's, never a close that goes to the
  // tray and is lost.
  for (const s of ['SIGTERM', 'SIGINT', 'SIGHUP']) process.on(s, quitNow);
  app.whenReady().then(() => {
    // Woken or unlocked (0.10; the owner, 2026-10-30: the ThinkPad's player came back a minute after its Wi-Fi): the
    // page reconnects at once instead of waiting out a dead connection or its retry.
    for (const e of ['resume', 'unlock-screen'])
      powerMonitor.on(e, () => {
        win?.webContents.send('wake');
        hearPage();
      });
    session.defaultSession.setPermissionRequestHandler((_wc, permission, done, details) => done(ALLOWED.has(permission) && sameOrigin(details.requestingUrl)));
    session.defaultSession.setPermissionCheckHandler((_wc, permission, requesting) => ALLOWED.has(permission) && sameOrigin(requesting));
    // Every request of the server's kind (https, or http at a home address) comes here: the app's own page answers on
    // Cratarr's address where it is the one served, the rest goes on as it came.
    if (origin) session.defaultSession.protocol.handle(new URL(origin).protocol.slice(0, -1), (req) => (sameOrigin(req.url) ? page.answer(req, sendOn) : sendOn(req)));
    hearPage();
    setInterval(hearPage, 10 * 60_000);
    tray = new Tray(nativeImage.createFromPath(ICON).resize({ width: 24, height: 24 }));
    tray.setToolTip('Cratarr');
    tray.on('click', show);
    menus();
    createWindow();
  });
  // Without a tray (a desktop that has none), closing the window ends it.
  app.on('window-all-closed', () => app.quit());
}
