<script lang="ts" generics="T extends string | boolean">
  // A choice in a row (the owner, 2026-11-02: a row never carries a pill): its name, its value and ›, and a tap opens
  // the one-of sheet (lib/ChoiceList's rows, the round mark at the end), which closes on a choice. Any choice of two or
  // more; on or off is a switch instead.
  import ChoiceList from '../ChoiceList.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import Item from './Item.svelte';
  import type { IconName } from '../Icon.svelte';

  let {
    name,
    line = null,
    more = null,
    disabled = false,
    options,
    value,
    onchange,
  }: {
    name: string;
    line?: string | null;
    more?: string | null;
    disabled?: boolean;
    options: readonly { value: T; label: string; line?: string | null; icon?: IconName }[];
    value: T;
    onchange: (value: T) => void;
  } = $props();

  let open = $state(false);
  const shown = $derived(options.find((o) => o.value === value)?.label ?? null);
</script>

<Item {name} {line} {more} {disabled} value={shown} onclick={disabled ? null : () => (open = true)} />
{#if open}
  <MenuSheet label={name} title={name} onclose={() => (open = false)}>
    <div class="choices">
      <ChoiceList
        label={name}
        options={[...options]}
        {value}
        onchange={(v) => {
          onchange(v);
          open = false;
        }}
      />
    </div>
  </MenuSheet>
{/if}

<style>
  .choices {
    padding: 0 8px 8px;
    overflow-y: auto;
  }
</style>
