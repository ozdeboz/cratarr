// After the packages are made (0.9.18): a .deb installs its files with the modes it carries, so one writable by group
// or others lets any user of the computer change the app. after-pack.js fixes what electron-builder lays out, but not
// what it adds around it (the icons, copied with their checkout's mode: 0664 under umask 0002); this reads each .deb's
// list and stops the build at any such entry (links aside: their mode means nothing).
'use strict';

const { execFileSync } = require('node:child_process');

exports.default = async function afterAllArtifactBuild(result) {
  const bad = [];
  for (const file of result.artifactPaths.filter((f) => f.endsWith('.deb'))) {
    for (const line of execFileSync('dpkg-deb', ['-c', file], { encoding: 'utf8' }).split('\n')) {
      const mode = line.slice(0, 10);
      if (!mode || mode[0] === 'l') continue;
      if (mode[5] === 'w' || mode[8] === 'w') bad.push(`${file}: ${mode} ${line.split(/\s+/).slice(5).join(' ')}`);
    }
  }
  if (bad.length) throw new Error(`Writable by group or others in the package:\n${bad.join('\n')}`);
  return [];
};
