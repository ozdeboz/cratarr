<script lang="ts">
  // A directory (LDAP) asked first at sign-in (decision 0040), Cratarr's own account staying the fallback. Shown, never
  // set here: the operator writes ldap.json in the data folder (a page that could set the address or the password file
  // could send a file's contents anywhere, or sign anyone in). The service account's password: only whether it is set.
  import { getDirectory, type DirectorySettings } from '../api';
  import MoreInfo from '../MoreInfo.svelte';

  let d = $state<DirectorySettings | null>(null);
  $effect(() => {
    getDirectory().then(
      (r) => (d = r),
      () => (d = null),
    );
  });
  const others = $derived(d ? Object.entries(d.config.accounts ?? {}) : []);
</script>

{#if d?.config.enabled}
  <section aria-labelledby="dir-h">
    <h3 id="dir-h">
      Directory <MoreInfo
        label="Directory"
        text={"Asked first at sign-in; Cratarr's own accounts stay a way in.\n\nSet by whoever runs Cratarr, in ldap.json in its data folder, and never here: a page that could set it could send a file's contents anywhere, or sign anyone in."}
      />
    </h3>
    <dl class="account">
      <dt>Address</dt>
      <dd><code>{d.config.url}</code>{#if d.config.starttls} · StartTLS{/if}</dd>
      <dt>Users under</dt>
      <dd><code>{d.config.base_dn}</code></dd>
      {#if d.config.group}
        <dt>Member of</dt>
        <dd><code>{d.config.group}</code></dd>
      {/if}
      <dt>Service account</dt>
      <dd>
        <code>{d.config.bind_dn}</code>
        <small class={d.bind_password === 'set' ? 'ok' : 'warn'}>{d.bind_password}</small>
      </dd>
      {#if others.length}
        <dt>Other names</dt>
        <dd>{#each others as [from, to], i (from)}{#if i}, {/if}{from} → {to}{/each}</dd>
      {/if}
    </dl>
  </section>
{/if}

<style>
  code {
    overflow-wrap: anywhere;
  }

  small {
    margin-left: 8px;
  }

  small.ok {
    color: var(--muted);
  }

  small.warn {
    color: var(--danger, #c33);
  }
</style>
