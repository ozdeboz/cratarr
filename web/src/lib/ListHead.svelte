<script lang="ts" generics="K">
  // A list's header (Symfonium's, decided 2026-10-15): its order (a tap on it chooses the field in a sheet, the arrow
  // turns it round), how many, and the select icon when the list has a select mode. The panel's track lists and every
  // page's list have it, and only it sets the order and direction (decision 0060). A field counted by use (Plays, Last
  // played, …) starts most first when chosen.
  import Icon from './Icon.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';

  let {
    fields,
    value = $bindable(),
    down = $bindable(false),
    count,
    selecting = null,
    onselect,
    open = $bindable(false),
    countOnly = false,
  }: {
    /** `most`: chosen, it starts most first (lib/pageView OrderField). */
    fields: readonly { value: K; label: string; most?: true }[];
    value: K;
    /** Turned round: descending. */
    down?: boolean;
    /** How many, in words ("12 tracks"). */
    count: string;
    /** Select mode on or off; null when the list has none. */
    selecting?: boolean | null;
    onselect?: () => void;
    /** The order's sheet is open (a view closing on Esc leaves it be meanwhile). */
    open?: boolean;
    /** How many alone, no order or select (a Library list as Flow, the owner 2026-10-26). */
    countOnly?: boolean;
  } = $props();

  const label = $derived(fields.find((f) => f.value === value)?.label ?? fields[0]?.label ?? '');
</script>

<div class="listhead">
  {#if !countOnly}
    <button class="sortby" onclick={() => (open = true)}>{label}</button>
    <button class="icon" aria-label={down ? 'Descending' : 'Ascending'} title={down ? 'Descending' : 'Ascending'} onclick={() => (down = !down)}
      ><Icon name={down ? 'arrowDown' : 'arrowUp'} size={20} /></button
    >
  {/if}
  <span class="grow"></span>
  <span class="count">{count}</span>
  {#if selecting !== null && !countOnly}
    <button class="icon" class:on={selecting} aria-pressed={selecting} aria-label="Start multi-select" title="Start multi-select" onclick={onselect}
      ><Icon name="listChecks" size={22} /></button
    >
  {/if}
</div>
{#if open}
  <MenuSheet label="Order" title="Order" onclose={() => (open = false)}>
    {#each fields as f (f.label)}
      <MenuEntry
        icon={f.value === value ? 'circleCheck' : 'circle'}
        label={f.label}
        onclick={() => {
          if (f.value !== value) down = !!f.most;
          value = f.value;
          open = false;
        }}
      />
    {/each}
  </MenuSheet>
{/if}

<style>
  .listhead {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 8px 4px 16px;
    color: var(--muted);
    font-size: 15px;
  }

  .sortby {
    padding: 8px 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .icon.on {
    color: var(--art-accent, var(--accent));
  }

  .grow {
    flex: 1;
  }

  .count {
    padding-right: 4px;
  }
</style>
