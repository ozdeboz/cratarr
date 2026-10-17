<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"): the account Subsonic apps and Cratarr's sign-in page (decision
  // 0039) sign in with; whether every request needs a sign-in (decision 0043); the devices signed in, each to sign out;
  // the API keys for what cannot sign in on a page (Home Assistant, scripts), each to revoke. Switching the requirement
  // and making a key ask for the password again. Acts at once.
  import {
    getApiKeys,
    getSignedIn,
    getSignInRequired,
    newApiKey,
    revokeApiKey,
    setSignInRequired,
    signOutDevice,
    type ApiKey,
    type SignedIn,
  } from '../api';
  import Switch from '../Switch.svelte';
  import { ed } from './editor.svelte';
  import DirectoryPart from './DirectoryPart.svelte';

  let devices = $state<SignedIn[]>([]);
  let keys = $state<ApiKey[]>([]);
  let required = $state<boolean | null>(null);
  let error = $state<string | null>(null);

  const load = () => {
    getSignedIn().then(
      (r) => (devices = r.sessions),
      () => (devices = []),
    );
    getApiKeys().then(
      (r) => (keys = r.keys),
      () => (keys = []),
    );
    getSignInRequired().then(
      (r) => (required = r.required),
      () => (required = null),
    );
  };
  // On opening, and after a new password (which signs the other devices out).
  $effect(() => {
    if (!ed.sub.busy) load();
  });

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  async function signOut(d: SignedIn) {
    error = null;
    try {
      await signOutDevice(d.id);
      load();
    } catch (e) {
      failed(e);
    }
  }

  // The requirement: flipping it asks for the password, then switches.
  let switching = $state<boolean | null>(null);
  let switchPassword = $state('');
  let busy = $state(false);
  async function confirmSwitch() {
    if (switching === null) return;
    error = null;
    busy = true;
    try {
      required = (await setSignInRequired(switching, switchPassword)).required;
      switching = null;
    } catch (e) {
      failed(e);
    } finally {
      switchPassword = '';
      busy = false;
    }
  }

  // A new key: its name and the password; the key is shown here once.
  let keyName = $state('');
  let keyPassword = $state('');
  let made = $state<{ name: string; key: string } | null>(null);
  async function makeKey() {
    error = null;
    busy = true;
    try {
      const k = await newApiKey(keyName.trim(), keyPassword);
      made = { name: k.name, key: k.key };
      keyName = '';
      load();
    } catch (e) {
      failed(e);
    } finally {
      keyPassword = '';
      busy = false;
    }
  }

  async function revoke(k: ApiKey) {
    error = null;
    try {
      await revokeApiKey(k.id);
      if (made && made.name === k.name) made = null;
      load();
    } catch (e) {
      failed(e);
    }
  }

  // When it was last used: the time today, else the day.
  function when(s: number) {
    const d = new Date(s * 1000);
    return d.toDateString() === new Date().toDateString()
      ? d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
      : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  }
  const accounts = $derived(new Set(devices.map((d) => d.user)).size);
</script>

{#if ed.sub.accounts}
  <section aria-labelledby="auth-h">
    <h2 id="auth-h">Authentication</h2>
    <p class="lede">Your account for signing in to Cratarr, and for Subsonic apps (Symfonium, Feishin…) at this server address: <code>{location.origin}</code></p>
    {#if ed.sub.accounts.length === 0}
      <div class="fields">
        <label>
          <span class="l">Username</span>
          <input type="text" bind:value={ed.sub.user} spellcheck="false" autocomplete="username" autocapitalize="off" />
        </label>
        <label>
          <span class="l">Password</span>
          <input type="password" bind:value={ed.sub.password} autocomplete="new-password" />
        </label>
      </div>
      <div class="row">
        <button class="primary" disabled={ed.sub.busy || !ed.sub.user.trim() || !ed.sub.password} aria-busy={ed.sub.busy} onclick={ed.createAccount}>Create account</button>
      </div>
    {:else}
      <dl class="account">
        <dt>Account</dt>
        <dd>{ed.sub.accounts.join(', ')}</dd>
      </dl>
      <div class="fields">
        <label>
          <span class="l">Current password</span>
          <input type="password" bind:value={ed.sub.current} autocomplete="current-password" />
        </label>
        <label>
          <span class="l">New password</span>
          <input type="password" bind:value={ed.sub.password} autocomplete="new-password" />
        </label>
      </div>
      <div class="row">
        <button disabled={ed.sub.busy || !ed.sub.current || !ed.sub.password} aria-busy={ed.sub.busy} onclick={ed.changePassword}>Change password</button>
      </div>

      {#if required !== null}
        <div class="pref">
          <Switch label="Sign-in required everywhere" checked={switching ?? required} disabled={busy} onchange={(on) => (switching = on === required ? null : on)} />
        </div>
        {#if switching !== null}
          <form class="confirm" onsubmit={(e) => (e.preventDefault(), confirmSwitch())}>
            <input type="password" bind:value={switchPassword} placeholder="Password" autocomplete="current-password" aria-label="Password" />
            <button class="primary" disabled={busy || !switchPassword} aria-busy={busy}>{switching ? 'Switch on' : 'Switch off'}</button>
            <button type="button" onclick={() => ((switching = null), (switchPassword = ''))}>Cancel</button>
          </form>
        {/if}
      {/if}

      {#if devices.length}
        <h3 id="signed-in-h">Signed in</h3>
        <ul class="list" aria-labelledby="signed-in-h">
          {#each devices as d (d.id)}
            <li>
              <span class="what">{d.device}{#if accounts > 1}<small>{d.user}</small>{/if}</span>
              <span class="when">{d.current ? 'This device' : when(d.last_used)}</span>
              {#if d.current}
                <!-- Signing out here clears this browser's cookie too. -->
                <form method="post" action="/signout"><button>Sign out</button></form>
              {:else}
                <button onclick={() => signOut(d)}>Sign out</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <h3 id="keys-h">API keys</h3>
      {#if keys.length}
        <ul class="list" aria-labelledby="keys-h">
          {#each keys as k (k.id)}
            <li>
              <span class="what">{k.name}</span>
              <span class="when">{k.last_used ? when(k.last_used) : 'Not used yet'}</span>
              <button onclick={() => revoke(k)}>Revoke</button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if made}
        <div class="made">
          <code>{made.key}</code>
          <button onclick={() => navigator.clipboard?.writeText(made?.key ?? '')}>Copy</button>
        </div>
      {/if}
      <form class="confirm" onsubmit={(e) => (e.preventDefault(), makeKey())}>
        <input type="text" bind:value={keyName} placeholder="Name" aria-label="Key name" autocomplete="off" spellcheck="false" />
        <input type="password" bind:value={keyPassword} placeholder="Password" aria-label="Password" autocomplete="current-password" />
        <button disabled={busy || !keyName.trim() || !keyPassword} aria-busy={busy}>New key</button>
      </form>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <DirectoryPart />
    {/if}
  </section>
{/if}

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .what {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .what small {
    margin-left: 8px;
    color: var(--muted);
  }

  .when {
    color: var(--muted);
    white-space: nowrap;
  }

  .list form {
    display: contents;
  }

  .confirm {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 8px 0;
  }

  .confirm input {
    flex: 1 1 140px;
    min-width: 0;
  }

  .made {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .made code {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    user-select: all;
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
