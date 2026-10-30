// What the page sees of the desktop app (decision 0078), on `window.CratarrDesktop`: the app's version and the
// computer's name (the player's first name). There the page counts as installed (Downloads) and plays as in a browser.
// The setup page alone also gets the server to read and change.
'use strict';

const { contextBridge, ipcRenderer } = require('electron');

const arg = (name) => process.argv.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3) ?? null;

contextBridge.exposeInMainWorld('CratarrDesktop', {
  version: arg('cratarr-desktop'),
  name: arg('cratarr-desktop-name'),
  // The computer woke or was unlocked (main.js): the page reconnects at once (0.10).
  onWake: (f) => ipcRenderer.on('wake', () => f()),
  ...(location.protocol === 'file:'
    ? { server: () => ipcRenderer.invoke('server'), connect: (address) => ipcRenderer.invoke('connect', address) }
    : {}),
});
