// The browser end-to-end tests' two invented accounts (scripts/browser-e2e.sh): `ann`, an administrator (a server with
// no accounts takes any caller as one), and `bob`, made by her once sign-in is asked. Their passwords are made here
// and written to the file named, one `name password` a line; never printed.
//
//   node e2e/accounts.mjs <server> <file>
import { randomBytes } from 'node:crypto';
import { writeFileSync } from 'node:fs';

const [server, file] = process.argv.slice(2);
if (!server || !file) throw new Error('usage: accounts.mjs <server> <file>');
const accounts = [
  { name: 'ann', admin: true },
  { name: 'bob', admin: false },
].map((a) => ({ ...a, password: randomBytes(18).toString('base64url') }));
const headers = { 'Content-Type': 'application/json', Origin: server };

async function add(a, cookie) {
  const r = await fetch(`${server}/api/users`, { method: 'POST', headers: { ...headers, ...(cookie ? { Cookie: cookie } : {}) }, body: JSON.stringify(a) });
  if (!r.ok) throw new Error(`${a.name} not made: ${r.status}`);
}

await add(accounts[0]);
const signIn = await fetch(`${server}/signin`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/x-www-form-urlencoded', Origin: server },
  body: new URLSearchParams({ username: accounts[0].name, password: accounts[0].password }),
  redirect: 'manual',
});
const cookie = (signIn.headers.getSetCookie?.() ?? []).map((c) => c.split(';')[0]).join('; ');
if (!cookie) throw new Error(`${accounts[0].name} not signed in: ${signIn.status}`);
await add(accounts[1], cookie);
writeFileSync(file, accounts.map((a) => `${a.name} ${a.password}`).join('\n') + '\n', { mode: 0o600 });
console.log(`accounts made: ${accounts.map((a) => a.name).join(', ')}`);
