// Before a build (the owner, 2026-10-29: no hard-coded IP addresses or other house-specific endpoints in builds): the
// app's own files, as they go into the package, may hold no private IPv4 address and no host name only a home network
// resolves. A house's own names (its machines, its domains) come from CRATARR_HOUSE_PATTERNS, comma-separated regular
// expressions set where the build runs (a repository secret), so they never sit in this public repository. A hit is
// named by file and line only: a public build's log shows no more of it.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

/** What the package takes of this folder besides the icons (package.json's build.files). */
const FILES = ['main.js', 'preload.js', 'setup.html', 'package.json'];
const PATTERNS = [
  // RFC 1918's ranges and link-local; RFC 5737's documentation ones (192.0.2.0/24 and the like) are fine.
  /\b(10\.\d{1,3}\.\d{1,3}\.\d{1,3}|172\.(1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}|169\.254\.\d{1,3}\.\d{1,3})\b/,
  /\b[a-z0-9-]+\.(fritz\.box|local|lan|home\.arpa|internal)\b/i,
  ...(process.env.CRATARR_HOUSE_PATTERNS ?? '')
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => new RegExp(p, 'i')),
];

let hits = 0;
for (const name of FILES) {
  fs.readFileSync(path.join(__dirname, name), 'utf8')
    .split('\n')
    .forEach((line, i) => {
      if (!PATTERNS.some((p) => p.test(line))) return;
      hits++;
      console.error(`${name}:${i + 1}: a house-specific address or name`);
    });
}
if (hits) {
  console.error(`${hits} in the app's own files: the build stops.`);
  process.exit(1);
}
