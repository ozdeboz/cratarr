<script lang="ts">
  // Account (decision 0051, several users): who is signed in and their role, both from the directory and read-only;
  // their own password for OpenSubsonic apps (whose sign-in needs the password itself, which a directory never gives);
  // their devices (each plays only on its own) and their sign-ins, each to end. Acts at once.
  import { endMySession, removeMyDevice, setAppPassword, type OwnDevice, type UserSession } from '../api';
  import { loadMe, me } from '../me.svelte';
  import MoreInfo from '../MoreInfo.svelte';
  import { toast } from '../toast.svelte';
  import { when } from './when';

  let busy = $state(false);

  // The app password: its field only once asked for.
  let setting = $state(false);
  let password = $state('');
  async function savePassword() {
    busy = true;
    try {
      await setAppPassword(password);
      setting = false;
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not set the password');
    } finally {
      password = '';
      busy = false;
    }
  }

  async function end(s: UserSession) {
    try {
      await endMySession(s.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not sign it out');
    }
  }

  async function remove(d: OwnDevice) {
    try {
      await removeMyDevice(d.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not remove it');
    }
  }
</script>

<section aria-labelledby="account-h">
  <h2 id="account-h">Account</h2>
  <dl class="account">
    {#if me.name}<dt>Name</dt><dd>{me.name}</dd>{/if}
    <dt>Role</dt>
    <dd>{me.admin ? 'Administrator' : 'User'}</dd>
    <dt>Address</dt>
    <dd><code>{location.origin}</code></dd>
  </dl>

  {#if me.name}
    <div class="group">
      <h3>
        App password <MoreInfo label="App password" text="OpenSubsonic apps sign in with your name and this password, never your directory password." />
      </h3>
    </div>
    {#if setting}
      <form class="confirm" onsubmit={(e) => (e.preventDefault(), savePassword())}>
        <input type="password" bind:value={password} placeholder="New password" aria-label="New password" autocomplete="new-password" />
        <button class="primary" disabled={busy || !password} aria-busy={busy}>Save</button>
        <button type="button" disabled={busy} onclick={() => ((setting = false), (password = ''))}>Cancel</button>
      </form>
    {:else}
      <div class="row"><button onclick={() => (setting = true)}>{me.app_password ? 'Change app password' : 'Set app password'}</button></div>
    {/if}
  {/if}

  {#if me.devices?.length}
    <div class="group"><h3 id="devices-h">Devices</h3></div>
    <ul class="list" aria-labelledby="devices-h">
      {#each me.devices as d (d.id)}
        <li>
          <span class="what">{d.name}<small>{d.kind}</small></span>
          {#if d.last_seen}<span class="when">{when(d.last_seen)}</span>{/if}
          <button onclick={() => remove(d)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if me.sessions?.length}
    <div class="group"><h3 id="sessions-h">Signed in</h3></div>
    <ul class="list" aria-labelledby="sessions-h">
      {#each me.sessions as s (s.id)}
        <li>
          <span class="what">{s.device}</span>
          <span class="when">{s.current ? 'This device' : when(s.last_used)}</span>
          {#if s.current}
            <!-- Signing out here clears this browser's cookie too. -->
            <form method="post" action="/signout"><button>Sign out</button></form>
          {:else}
            <button onclick={() => end(s)}>Sign out</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
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
</style>
