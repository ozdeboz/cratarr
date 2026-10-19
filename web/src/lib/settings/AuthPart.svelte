<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"), its administrators' since several users (decision 0051):
  // Cratarr's own account, an administrator apart from the directory; whether every request needs a sign-in (decision
  // 0043); the API keys for what cannot sign in on a page (Home Assistant, scripts), each acting for its owner, only
  // reading or also in control, each to revoke; the directory. Switching the requirement and making a key ask for the
  // password again. Acts at once. Each user's sign-ins are under Account and Users.
  import { getApiKeys, getSignInRequired, getUsers, newApiKey, revokeApiKey, setSignInRequired, type ApiKey, type KeyScope } from '../api';
  import { me } from '../me.svelte';
  import MoreInfo from '../MoreInfo.svelte';
  import Switch from '../Switch.svelte';
  import { ed } from './editor.svelte';
  import DirectoryPart from './DirectoryPart.svelte';
  import { when } from './when';

  let keys = $state<ApiKey[]>([]);
  let required = $state<boolean | null>(null);
  let error = $state<string | null>(null);

  // Whom a key can act for: every user.
  let owners = $state<string[]>([]);
  const load = () => {
    getUsers().then(
      (u) => (owners = u.filter((x) => x.active).map((x) => x.name)),
      () => (owners = []),
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
  // On opening, and after a new password.
  $effect(() => {
    if (!ed.sub.busy) load();
  });

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

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

  // A new key: its name, whom it acts for (oneself unless chosen), what it may do, and the password; the key is shown
  // here once.
  let keyName = $state('');
  let keyOwner = $state('');
  let keyScope = $state<KeyScope>('control');
  let keyPassword = $state('');
  let made = $state<{ name: string; key: string } | null>(null);
  async function makeKey() {
    error = null;
    busy = true;
    try {
      const k = await newApiKey(keyName.trim(), keyPassword, keyOwner || undefined, keyScope);
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


  // The password's fields only once asked for (deeper only while it applies): a new password, then back.
  let changing = $state(false);
  async function changePassword() {
    await ed.changePassword();
    if (!ed.sub.current && !ed.sub.password) changing = false;
  }
</script>

{#if ed.sub.accounts}
  <section aria-labelledby="auth-h">
    <h2 id="auth-h">Authentication</h2>
    <p class="lede">
      Cratarr's own administrator account, apart from the directory <MoreInfo
        label="Authentication"
        text="Sign in to these pages with it. Subsonic apps take this address with this account's own name and password (never a directory's)."
      />
    </p>
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
        <dt>Address</dt>
        <dd><code>{location.origin}</code></dd>
      </dl>
      {#if changing}
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
          <button class="primary" disabled={ed.sub.busy || !ed.sub.current || !ed.sub.password} aria-busy={ed.sub.busy} onclick={changePassword}>Change password</button>
          <button disabled={ed.sub.busy} onclick={() => ((changing = false), (ed.sub.current = ''), (ed.sub.password = ''))}>Cancel</button>
        </div>
      {:else}
        <div class="row"><button onclick={() => (changing = true)}>Change password</button></div>
      {/if}

      {#if required !== null}
        <div class="pref">
          <Switch
            label="Sign-in required everywhere"
            more={"On, every page and API request needs a sign-in, an API key or a signed link, on the home network too. Off, Cratarr asks none itself: a proxy in front of it may. Subsonic apps always sign in with their name and password.\n\nSwitching it asks for the password again."}
            checked={switching ?? required}
            disabled={busy}
            onchange={(on) => (switching = on === required ? null : on)}
          />
        </div>
        {#if switching !== null}
          <form class="confirm" onsubmit={(e) => (e.preventDefault(), confirmSwitch())}>
            <input type="password" bind:value={switchPassword} placeholder="Password" autocomplete="current-password" aria-label="Password" />
            <button class="primary" disabled={busy || !switchPassword} aria-busy={busy}>{switching ? 'Switch on' : 'Switch off'}</button>
            <button type="button" onclick={() => ((switching = null), (switchPassword = ''))}>Cancel</button>
          </form>
        {/if}
      {/if}

      <div class="group">
        <h3 id="keys-h">
          API keys <MoreInfo label="API keys" text="For what cannot sign in on a page: Home Assistant, scripts. A key shows once, when it is made; revoking one ends it at once." />
        </h3>
      </div>
      {#if keys.length}
        <ul class="list" aria-labelledby="keys-h">
          {#each keys as k (k.id)}
            <li>
              <span class="what">{k.name}{#if k.owner}<small>{k.owner}{k.scope === 'read' ? ' · Read only' : ''}</small>{/if}</span>
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
        {#if owners.length > 1}
          <select bind:value={keyOwner} aria-label="Acts for">
            {#each owners as o (o)}<option value={o === me.name ? '' : o}>{o}</option>{/each}
          </select>
        {/if}
        <select bind:value={keyScope} aria-label="Scope">
          <option value="control">Control</option>
          <option value="read">Read only</option>
        </select>
        <input type="password" bind:value={keyPassword} placeholder="Password" aria-label="Password" autocomplete="current-password" />
        <button disabled={busy || !keyName.trim() || !keyPassword} aria-busy={busy}>New key</button>
      </form>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <DirectoryPart />
    {/if}
  </section>
{/if}

<style>
  /* Each group (Signed in, API keys) apart from the one above it: its heading's own room, which the panel's heading
     rule leaves alone. */
  .group {
    padding: 24px 0 4px;
  }

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

  .confirm select {
    flex: 0 1 auto;
    width: auto;
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
