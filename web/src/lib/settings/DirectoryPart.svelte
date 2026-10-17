<script lang="ts">
  // A directory (LDAP) asked first at sign-in (decision 0040), Cratarr's own account staying the fallback. Saved and
  // tested here at once, outside the Save bar; the service account's password stays in its file, only "set" shown.
  import { getDirectory, saveDirectory, testDirectory, type DirectoryConfig, type DirectorySettings } from '../api';
  import Switch from '../Switch.svelte';

  let saved = $state<DirectorySettings | null>(null);
  let c = $state<DirectoryConfig | null>(null);
  /** Directory names that sign in as a differently named account, one `name = account` a line. */
  let mapping = $state('');
  let lookup = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let result = $state<string | null>(null);

  const toText = (m: Record<string, string>) =>
    Object.entries(m)
      .map(([k, v]) => `${k} = ${v}`)
      .join('\n');
  const fromText = (t: string) =>
    Object.fromEntries(
      t
        .split('\n')
        .map((l) => l.split('=').map((x) => x.trim()))
        .filter((p) => p.length === 2 && p[0] && p[1]),
    );

  const take = (s: DirectorySettings) => {
    saved = s;
    c = { ...s.config };
    mapping = toText(s.config.accounts ?? {});
  };
  $effect(() => {
    getDirectory().then(take, () => {});
  });

  const current = (): DirectoryConfig => ({ ...(c as DirectoryConfig), accounts: fromText(mapping) });
  const changed = $derived(!!c && !!saved && JSON.stringify(current()) !== JSON.stringify(saved.config));

  async function save(next: DirectoryConfig) {
    busy = true;
    error = null;
    try {
      take(await saveDirectory(next));
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function test() {
    busy = true;
    error = null;
    result = null;
    try {
      const r = await testDirectory(current(), lookup.trim());
      if (!r.ok) error = r.error ?? 'The directory cannot be reached.';
      else result = r.found == null ? 'Connected' : r.found ? `${lookup.trim()}: found` : `${lookup.trim()}: not found`;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if c}
  <section aria-labelledby="dir-h">
    <h3 id="dir-h">Directory</h3>
    <div class="pref">
      <Switch
        label="Sign in with a directory (LDAP)"
        hint="Asked first; this account stays the fallback"
        checked={c.enabled}
        onchange={(on) => {
          if (c) {
            c.enabled = on;
            if (!on) save(current());
          }
        }}
      />
    </div>
    {#if c.enabled}
      <div class="fields">
        <label>
          <span class="l">Address</span>
          <input type="url" bind:value={c.url} placeholder="ldaps://ldap.example.com:636" spellcheck="false" autocapitalize="off" />
        </label>
        <div class="pref">
          <Switch label="StartTLS" hint="Plain LDAP only on this machine's networks" checked={c.starttls} onchange={(on) => c && (c.starttls = on)} />
        </div>
        <label>
          <span class="l">Users under</span>
          <input type="text" bind:value={c.base_dn} placeholder="ou=people,dc=example,dc=com" spellcheck="false" autocapitalize="off" />
        </label>
        <label>
          <span class="l">Name attribute</span>
          <input type="text" bind:value={c.user_attribute} placeholder="uid" spellcheck="false" autocapitalize="off" />
        </label>
        <label>
          <span class="l">Member of</span>
          <input type="text" bind:value={c.group} placeholder="cn=music,ou=groups,dc=example,dc=com" spellcheck="false" autocapitalize="off" />
        </label>
        <label>
          <span class="l">Service account</span>
          <input type="text" bind:value={c.bind_dn} placeholder="uid=cratarr,ou=people,dc=example,dc=com" spellcheck="false" autocapitalize="off" />
        </label>
        <label>
          <span class="l">Its password file</span>
          <input type="text" bind:value={c.bind_password_file} placeholder="/data/ldap-bind" spellcheck="false" autocapitalize="off" />
          {#if saved && c.bind_password_file === saved.config.bind_password_file}
            <small class={saved.bind_password === 'set' ? 'ok' : 'warn'}>{saved.bind_password}</small>
          {/if}
        </label>
        <label>
          <span class="l">Other names</span>
          <textarea bind:value={mapping} rows="2" placeholder="directory name = account" spellcheck="false" autocapitalize="off"></textarea>
        </label>
      </div>
      <div class="row">
        <button class="primary" disabled={busy || !changed} aria-busy={busy} onclick={() => save(current())}>Save</button>
        <input class="lookup" type="text" bind:value={lookup} placeholder="Name to look up" aria-label="Name to look up" spellcheck="false" autocapitalize="off" />
        <button disabled={busy || !c.url} onclick={test}>Test</button>
      </div>
      {#if result}<p class="result">{result}</p>{/if}
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
{/if}

<style>
  .row {
    flex-wrap: wrap;
  }

  .lookup {
    flex: 1;
    min-width: 8em;
  }

  textarea {
    font: inherit;
    resize: vertical;
  }

  small.ok {
    color: var(--muted);
  }

  small.warn,
  .error {
    color: var(--danger, #c33);
  }

  .result {
    color: var(--muted);
  }
</style>
