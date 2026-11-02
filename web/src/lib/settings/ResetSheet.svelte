<script lang="ts">
  // Reset, asked once more (the owner, 2026-11-01: a list, from what Reset does): what goes back, each one now → as it
  // came, then Reset and Cancel. One sheet for a part's Reset in the top right glass (lib/SettingsPanel) and each
  // section's (lib/settings/OrderReset, the player's extra details, a tape's settings: the owner, 2026-11-02, "The
  // section reset buttons should open the same panel").
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    rows,
    onconfirm,
    onclose,
  }: {
    title: string;
    rows: readonly { name: string; from: string; to: string }[];
    onconfirm: () => unknown;
    onclose: () => void;
  } = $props();

  let busy = $state(false);
  async function reset() {
    busy = true;
    try {
      await onconfirm();
      onclose();
    } finally {
      busy = false;
    }
  }
</script>

<MenuSheet label="Reset" icon="resetSettings" {title} {onclose}>
  <ul class="reset">{#each rows as r (r.name)}<li><span class="name">{r.name}</span><span class="to">{r.from} → {r.to}</span></li>{/each}</ul>
  <MenuEntry icon="resetSettings" label="Reset" danger {busy} disabled={busy} onclick={reset} />
  <MenuEntry icon="close" label="Cancel" disabled={busy} onclick={onclose} />
</MenuSheet>

<style>
  .reset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 4px 24px 12px;
    list-style: none;
  }

  .reset li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 2px 12px;
  }

  .reset .to {
    color: var(--muted);
  }
</style>
