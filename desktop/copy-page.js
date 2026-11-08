// The app's own copy of Cratarr's page (decision 0085, step 1; page.js answers with it): the web app's build (web/dist,
// as `pnpm build` leaves it) copied into page/ before a run or a build, which the package carries as resources/page.
// Only a build of this very release: without one, or with another release's, the app goes to the server's page as
// before, and says so as it is built; CRATARR_PAGE=required makes that a failure, for an app that is handed out.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const from = path.join(__dirname, '..', 'web', 'dist');
const to = path.join(__dirname, 'page');
const { version } = require('./package.json');
/** The backdrops' and skies' pictures (17 MB of the page's 22) stay the server's, as in the Android app: asked for
 *  there, then kept by the page's service worker, so only an app that never reached its server goes without them. */
const HEAVY = ['backdrops', 'skies'];

let release = null;
try {
  release = JSON.parse(fs.readFileSync(path.join(from, 'build.json'), 'utf8')).version ?? null;
} catch {
  /* no build */
}
const missing = !release ? 'web/dist holds no built page' : release !== version ? `web/dist is ${release}'s page, not ${version}'s` : null;

fs.rmSync(to, { recursive: true, force: true });
// The package takes the folder as it is: empty, the app has no page of its own.
fs.mkdirSync(to, { recursive: true });
if (missing) {
  const said = `This app carries no page of its own: ${missing}. Run \`pnpm build\` in web/ first.`;
  if (process.env.CRATARR_PAGE === 'required') {
    console.error(said);
    process.exit(1);
  }
  console.warn(said);
} else {
  fs.cpSync(from, to, { recursive: true, filter: (src) => !HEAVY.includes(path.relative(from, src)) });
}
