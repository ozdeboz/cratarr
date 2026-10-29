// The app's icons are the web app's (web/public/icons), copied in before a run or a build: one set to keep.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const from = path.join(__dirname, '..', 'web', 'public', 'icons');
const to = path.join(__dirname, 'icons');
fs.mkdirSync(to, { recursive: true });
for (const name of ['icon-192.png', 'icon-512.png']) fs.copyFileSync(path.join(from, name), path.join(to, name));
