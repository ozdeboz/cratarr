// After electron-builder lays the app out and before it packs it (decision 0078): Electron's own files come out of its
// archive writable by anyone (0777, 0666), and a package installs them as they are, so any user of the computer could
// swap the binary. Here they are writable by their owner alone.
'use strict';

const { execFileSync } = require('node:child_process');

exports.default = async function afterPack(context) {
  if (context.electronPlatformName === 'linux') execFileSync('chmod', ['-R', 'go-w', context.appOutDir]);
};
