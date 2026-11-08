// What the browser end-to-end tests share (scripts/browser-e2e.sh starts what they need and names it): the page's
// address (E2E_PAGE), the invented accounts' file (E2E_ACCOUNTS: `name password` a line, never printed), and a
// Chromium already on the machine (CHROMIUM; playwright-core downloads none).
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright-core';

export const PAGE = process.env.E2E_PAGE ?? 'http://127.0.0.1:5400';
const passwords = process.env.E2E_ACCOUNTS
  ? Object.fromEntries(
      readFileSync(process.env.E2E_ACCOUNTS, 'utf8')
        .trim()
        .split('\n')
        .map((l) => l.trim().split(' ')),
    )
  : {};

/** One browser and one page in it: one device. Errors the page throws are kept, for the test to say. `args`: the
 *  browser's own (sound without a press, say). */
export async function device(args = []) {
  const browser = await chromium.launch({ args, ...(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {}) });
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  return { browser, page, errors };
}

/** `user` signs in through Cratarr's own sign-in page. `at`: the address gone to first: the page finds out it has no session;
 *  the sign-in page itself, as a server sends one there with no page run at all. */
export async function signInOnly(page, user, at = '/') {
  await page.goto(`${PAGE}${at}`);
  await page.waitForURL(/\/signin/, { timeout: 20_000 });
  await page.fill('input[name=username]', user);
  await page.fill('input[name=password]', passwords[user]);
  await Promise.all([page.waitForURL((u) => !u.pathname.startsWith('/signin'), { timeout: 20_000 }), page.press('input[name=password]', 'Enter')]);
}

/** And the page has learnt whose it is (lib/account). */
export async function signIn(page, user, at = '/') {
  await signInOnly(page, user, at);
  await page.waitForFunction((u) => localStorage.getItem('cratarr-who') === JSON.stringify(u), user, { timeout: 20_000 });
}

/** Sign out as Settings › Account's does (lib/signOut), wherever it goes on to. */
export const askSignOut = (page) => inModule(page, '/src/lib/signOut.svelte.ts', (m) => void m.signOut(false));

/** Signs out, to Cratarr's sign-in page. */
export async function signOut(page) {
  await Promise.all([page.waitForURL(/\/signin/, { timeout: 20_000 }), askSignOut(page)]);
}

/** A page module's live copy (as the page loaded it, never a second one) does `fn(module, arg)` in the page. */
export function inModule(page, path, fn, arg) {
  return page.evaluate(
    async ({ path, fn, arg }) => {
      const url = performance.getEntriesByType('resource').map((e) => e.name).find((n) => new URL(n).pathname === path) ?? path;
      const m = await import(url);
      return new Function('m', 'arg', `return (${fn})(m, arg)`)(m, arg);
    },
    { path, fn: fn.toString(), arg },
  );
}

/** This browser's localStorage, every key with its value. */
export const storage = (page) => page.evaluate(() => Object.fromEntries(Object.keys(localStorage).map((k) => [k, localStorage.getItem(k)])));
