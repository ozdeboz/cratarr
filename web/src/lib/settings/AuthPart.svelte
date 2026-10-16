<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"): the account Subsonic apps and Cratarr's sign-in page away
  // from home (decision 0039) sign in with, and the devices signed in there, each to sign out; acts at once.
  import { getSignedIn, signOutDevice, type SignedIn } from '../api';
  import { ed } from './editor.svelte';

  let devices = $state<SignedIn[]>([]);
  let error = $state<string | null>(null);

  const load = () =>
    getSignedIn().then(
      (r) => (devices = r.sessions),
      () => (devices = []),
    );
  // On opening, and after a new password (which signs the other devices out).
  $effect(() => {
    if (!ed.sub.busy) load();
  });

  async function signOut(d: SignedIn) {
    error = null;
    try {
      await signOutDevice(d.id);
      load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const day = (s: number) => new Date(s * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  const accounts = $derived(new Set(devices.map((d) => d.user)).size);
</script>

{#if ed.sub.accounts}
  <section aria-labelledby="auth-h">
    <h2 id="auth-h">Authentication</h2>
    <p class="lede">Your account for signing in away from home, and for Subsonic apps (Symfonium, Feishin…) at this server address: <code>{location.origin}</code></p>
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
      {#if devices.length}
        <h3 id="signed-in-h">Signed in</h3>
        <ul class="devices" aria-labelledby="signed-in-h">
          {#each devices as d (d.id)}
            <li>
              <span class="what">{d.device}{#if accounts > 1}<small>{d.user}</small>{/if}</span>
              <span class="when">{d.current ? 'This device' : day(d.last_used)}</span>
              {#if d.current}
                <!-- Signing out here clears this browser's cookie too. -->
                <form method="post" action="/signout"><button>Sign out</button></form>
              {:else}
                <button onclick={() => signOut(d)}>Sign out</button>
              {/if}
            </li>
          {/each}
        </ul>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
      {/if}
    {/if}
  </section>
{/if}

<style>
  .devices {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .devices li {
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

  .devices form {
    display: contents;
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
