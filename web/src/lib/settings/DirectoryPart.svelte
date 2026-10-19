<script lang="ts">
  // A directory (LDAP) asked first at sign-in (decision 0040), Cratarr's own account staying the fallback. Shown, never
  // set here: the operator writes ldap.json in the data folder (a page that could set the address or the password file
  // could send a file's contents anywhere, or sign anyone in). The service account's password: only whether it is set.
  // A group of rows, each value in its line (the settings' North Star, 2026-10-19).
  import { getDirectory, type DirectorySettings } from '../api';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

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
  <Group
    title="Directory"
    more={"Asked first at sign-in; Cratarr's own accounts stay a way in.\n\nSet by whoever runs Cratarr, in ldap.json in its data folder, and never here: a page that could set it could send a file's contents anywhere, or sign anyone in."}
  >
    <Item name="Address" line={`${d.config.url}${d.config.starttls ? ' · StartTLS' : ''}`} wrap />
    <Item name="Users under" line={d.config.base_dn} wrap />
    {#if d.config.group}<Item name="Member of" line={d.config.group} wrap />{/if}
    <Item
      name="Service account"
      line={`${d.config.bind_dn} · password ${d.bind_password}`}
      tone={d.bind_password === 'set' ? null : 'warn'}
      wrap
    />
    {#if others.length}<Item name="Other names" line={others.map(([from, to]) => `${from} → ${to}`).join(', ')} wrap />{/if}
  </Group>
{/if}
