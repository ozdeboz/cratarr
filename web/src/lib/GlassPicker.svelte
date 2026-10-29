<script lang="ts" module>
  /** One choice; `group`: a heading it comes under (a menu's group), named once before the first of it. */
  export type PickerOption = { id: string; label: string; group?: string };
</script>

<script lang="ts">
  // What a page shows, on the glass beside back as the page's name (the owner, 2026-10-25): only the choice shown
  // ("Records ▾"); a tap grows the glass downwards with every choice, the one shown ticked; a choice (or a tap
  // elsewhere, or Esc) folds it. The Library's kind of record (lib/RecordsPicker) and Discover's mode
  // (lib/DiscoverPicker) are both this one.
  import GlassMenu from './GlassMenu.svelte';
  import Icon from './Icon.svelte';

  let {
    shown,
    menuLabel,
    options,
    chosen,
    onchoose,
  }: { shown: string; menuLabel: string; options: readonly PickerOption[]; chosen: string | null; onchoose: (id: string) => void } = $props();

  let open = $state(false);
  let el = $state<HTMLElement>();
  // Under the button, in the glass menu (lib/GlassMenu).
  let at = $state<{ top: number; left: number; minWidth: number }>({ top: 0, left: 0, minWidth: 0 });
  function place() {
    const r = el?.getBoundingClientRect();
    if (r) at = { top: r.bottom + 8, left: r.left - 12, minWidth: r.width + 30 };
  }
  function choose(id: string) {
    open = false;
    onchoose(id);
  }
</script>

<span class="views" class:open bind:this={el}>
  <button class="shown" aria-haspopup="listbox" aria-expanded={open} aria-label={`Shows ${shown.toLowerCase()}`} onclick={() => (place(), (open = !open))}
    >{shown}<Icon name="chevronDown" size={18} /></button
  >
  {#if open}
    <GlassMenu {at} keep={() => el} label={menuLabel} onclose={() => (open = false)}>
      {#each options as o, i (o.id)}
        {#if o.group && o.group !== options[i - 1]?.group}<span class="group" role="presentation">{o.group}</span>{/if}
        <button class="option" role="option" aria-selected={o.id === chosen} aria-label={o.group ? `${o.group} ${o.label.toLowerCase()}` : undefined} onclick={() => choose(o.id)}
          >{o.label}{#if o.id === chosen}<Icon name="check" size={18} />{/if}</button
        >
      {/each}
    </GlassMenu>
  {/if}
</span>

<style>
  .views {
    position: relative;
    display: flex;
  }

  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    min-height: 40px;
    padding: 0 6px 0 4px;
    border: 0;
    border-radius: 20px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 20px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }

  .shown :global(svg) {
    color: var(--muted);
    transition: transform 150ms ease;
  }

  .open .shown :global(svg) {
    transform: rotate(180deg);
  }

  /* A group's heading (Spotify's lists in Discover's), named once, as a menu's group. */
  .group {
    display: block;
    margin-top: 6px;
    padding: 8px 12px 2px;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 600;
  }
</style>
