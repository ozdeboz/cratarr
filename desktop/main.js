// Cratarr's desktop app (decision 0078), Linux first: Cratarr's own pages in a window of their own. Thin, as the
// Android app began (decision 0031): nothing of the web app is built in, the window shows the server's pages, and the
// page plays as it does in a browser. What the shell adds to an installed page:
// - it can start at login and keeps running when its window is closed (in the tray), so the computer stays one of
//   Cratarr's players;
// - it plays without a first click, and a hidden window keeps its timers (its reports, the next track);
// - it works with any server, the local http address too: that origin is treated as secure, so the page's service
//   worker, and with it Downloads, work there as on https.
'use strict';

const { app, BrowserWindow, Menu, Tray, ipcMain, nativeImage, powerMonitor, screen, session, shell } = require('electron');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

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

// Before the app is ready: what the page may do here that a browser tab may not.
app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
if (origin?.startsWith('http:')) app.commandLine.appendSwitch('unsafely-treat-insecure-origin-as-secure', origin);

/** One shell at a time: started again, it shows its window. */
const first = app.requestSingleInstanceLock();
if (!first) app.quit();

let win = null;
let tray = null;
let quitting = false;

const sameOrigin = (url) => {
  try {
    return !!origin && new URL(url).origin === origin;
  } catch {
    return false;
  }
};

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
    Menu.buildFromTemplate([{ label: 'Show Cratarr', click: show }, server, login, { type: 'separator' }, { label: 'Quit Cratarr', click: () => app.quit() }]),
  );
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      { label: 'Cratarr', submenu: [server, login, { type: 'separator' }, { role: 'quit' }] },
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
 *  visuals), a speaker of its choice. Nothing else: no camera, microphone or location. */
const ALLOWED = new Set(['clipboard-sanitized-write', 'fullscreen', 'screen-wake-lock', 'speaker-selection']);

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
    writeSettings({ bounds: win.getNormalBounds() });
    // Closed, it keeps playing and stays a player, in the tray; Quit (there or in the menu) ends it.
    if (!quitting && tray) {
      event.preventDefault();
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
  win.webContents.on('did-finish-load', () => (retry = 5_000));
  win.webContents.on('render-process-gone', (_event, details) => {
    if (details.reason !== 'clean-exit') win?.webContents.reload();
  });
  if (origin) win.loadURL(origin + '/');
  else win.loadFile(path.join(__dirname, 'setup.html'));
}

/** Windows: the app updates itself from Cratarr's GitHub releases (electron-updater; the owner, 2026-10-29), looking at
 *  start and every six hours, the update put in place when the app next quits. On Linux apt does it (the APT
 *  repository). */
function updates() {
  const { autoUpdater } = require('electron-updater');
  const look = () => autoUpdater.checkForUpdatesAndNotify().catch(() => {});
  look();
  setInterval(look, 6 * 3600_000);
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
  app.on('before-quit', () => (quitting = true));
  app.whenReady().then(() => {
    // Woken or unlocked (0.10; the owner, 2026-10-30: the ThinkPad's player came back a minute after its Wi-Fi): the
    // page reconnects at once instead of waiting out a dead connection or its retry.
    for (const e of ['resume', 'unlock-screen']) powerMonitor.on(e, () => win?.webContents.send('wake'));
    session.defaultSession.setPermissionRequestHandler((_wc, permission, done, details) => done(ALLOWED.has(permission) && sameOrigin(details.requestingUrl)));
    session.defaultSession.setPermissionCheckHandler((_wc, permission, requesting) => ALLOWED.has(permission) && sameOrigin(requesting));
    tray = new Tray(nativeImage.createFromPath(ICON).resize({ width: 24, height: 24 }));
    tray.setToolTip('Cratarr');
    tray.on('click', show);
    menus();
    createWindow();
    if (process.platform === 'win32' && app.isPackaged) updates();
  });
  // Without a tray (a desktop that has none), closing the window ends it.
  app.on('window-all-closed', () => app.quit());
}
