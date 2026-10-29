// The app's icons are the web app's (web/public/icons), copied in before a run or a build: one set to keep. A copy keeps
// its source's mode, which a checkout under umask 0002 makes group-writable, and the .deb installs it so: each is set
// to 0644 (check-deb.js stops a package that has one otherwise).
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const from = path.join(__dirname, '..', 'web', 'public', 'icons');
const to = path.join(__dirname, 'icons');
fs.mkdirSync(to, { recursive: true });
for (const name of ['icon-192.png', 'icon-512.png']) {
  fs.copyFileSync(path.join(from, name), path.join(to, name));
  fs.chmodSync(path.join(to, name), 0o644);
}
