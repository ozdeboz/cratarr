<script lang="ts">
  // Asked once more before something goes (a server, a folder, a Kodi, every exclusion put back): a sheet naming it,
  // the act in red, and Cancel, as the panel's Reset asks. `icon`: the act's (a bin by default).
  import type { IconName } from '../Icon.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    text = null,
    label,
    icon = 'trash',
    onconfirm,
    onclose,
  }: {
    title: string;
    text?: string | null;
    label: string;
    icon?: IconName;
    onconfirm: () => void;
    onclose: () => void;
  } = $props();
</script>

<MenuSheet label={title} {icon} {title} {onclose}>
  {#if text}<p class="text">{text}</p>{/if}
  <MenuEntry
    {icon}
    {label}
    danger
    onclick={() => {
      onconfirm();
      onclose();
    }}
  />
  <MenuEntry icon="close" label="Cancel" onclick={onclose} />
</MenuSheet>

<style>
  .text {
    margin: 0;
    padding: 4px 24px 12px;
    color: var(--muted);
  }
</style>
