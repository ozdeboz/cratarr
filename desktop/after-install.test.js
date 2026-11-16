// after-install.tpl's look at a running copy's command line (`cmdline_has`, `npm test`): Chromium writes its processes'
// command lines again as one string, the arguments joined by spaces, so the app's main process and its `--hidden` are
// told apart however the line is written.
'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const look = fs
  .readFileSync(path.join(__dirname, 'after-install.tpl'), 'utf8')
  .split('\n')
  .find((l) => l.startsWith('cmdline_has()'));

/** Whether a process whose command line is `cmdline` (as /proc keeps it) holds `arg`, by the template's own look. */
function has(cmdline, arg) {
  const proc = fs.mkdtempSync(path.join(os.tmpdir(), 'cratarr-proc-'));
  fs.writeFileSync(path.join(proc, 'cmdline'), cmdline);
  try {
    execFileSync('bash', ['-c', `${look}\ncmdline_has "$1" "$2"`, 'bash', proc, arg], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  } finally {
    fs.rmSync(proc, { recursive: true, force: true });
  }
}

test('the template has the look', () => assert.ok(look, 'cmdline_has() in after-install.tpl'));

test('a helper is told by its --type, however its command line is written', () => {
  assert.equal(has('/opt/Cratarr/cratarr --type=renderer --enable-crash-reporter=x\0', '--type'), true, 'as Chromium writes it again');
  assert.equal(has('/opt/Cratarr/cratarr\0--type=zygote\0--no-zygote-sandbox\0', '--type'), true, 'as the kernel keeps it');
  assert.equal(has('/opt/Cratarr/cratarr --hidden\0', '--type'), false, 'the main process');
  assert.equal(has('/opt/Cratarr/cratarr\0', '--type'), false, 'the main process, no arguments');
});

test('--hidden is seen however the command line is written, and only as itself', () => {
  assert.equal(has('/opt/Cratarr/cratarr --hidden\0', '--hidden'), true, 'as Chromium writes it again');
  assert.equal(has('/opt/Cratarr/cratarr\0--hidden\0', '--hidden'), true, 'as the kernel keeps it');
  assert.equal(has('/opt/Cratarr/cratarr --remote-debugging-port=9333\0', '--hidden'), false);
  assert.equal(has('/opt/Cratarr/cratarr --hiddenness\0', '--hidden'), false, 'another argument that begins so');
});
