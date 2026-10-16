<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"): the account Subsonic apps sign in with; acts at once.
  import { ed } from './editor.svelte';
</script>

{#if ed.sub.accounts}
  <section aria-labelledby="auth-h">
    <h2 id="auth-h">Authentication</h2>
    <p class="lede">Your account for Subsonic apps (Symfonium, Feishin…), at this server address: <code>{location.origin}</code></p>
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
    {/if}
  </section>
{/if}
