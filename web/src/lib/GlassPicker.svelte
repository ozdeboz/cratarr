<script lang="ts" module>
  import ChoiceMark from './ChoiceMark.svelte';
  /** One choice; `group`: a heading it comes under (a menu's group), named once before the first of it; `count`: how
   *  many it shows (the Library's types), at its end. */
  export type PickerOption = { id: string; label: string; group?: string; count?: number };
</script>

<script lang="ts">
  // What a page shows, on the glass beside back as the page's name (the owner, 2026-10-25): only the choice shown
  // ("Records ▾"); a tap grows the glass downwards with every choice, the one shown ticked; a choice (or a tap
  // elsewhere, or Esc) folds it. The Library's kind of record (lib/RecordsPicker) and Discover's mode
  // (lib/DiscoverPicker) are both this one. Where more than one can be chosen, each chosen is ticked, and a long-press
  // (`onhold`; with a mouse a right-click, as every long-press, and Ctrl-click or ⌘-click, the desktop's habit for
  // choosing several) takes one in or out without folding it (the owner, 2026-11-02, the Library's types).
  import GlassMenu from './GlassMenu.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';

  let {
    shown,
    menuLabel,
    options,
    chosen,
    onchoose,
    onhold = null,
  }: {
    shown: string;
    menuLabel: string;
    options: readonly PickerOption[];
    /** What is chosen: one, several (each ticked), or none. */
    chosen: string | readonly string[] | null;
    onchoose: (id: string) => void;
    /** A long-press on a choice: taken in or out of what is chosen, the menu staying open. */
    onhold?: ((id: string) => void) | null;
  } = $props();
  // One of many, the round mark (a tap chooses one); the square once several are chosen (by a long-press).
  const markKind = $derived(Array.isArray(chosen) && chosen.length > 1 ? 'several' : 'one');
  const ticked = (id: string) => (typeof chosen === 'string' ? chosen === id : !!chosen?.includes(id));

  let open = $state(false);
  // Counts in a column of their own, the tick's room kept beside them so they line up.
  const counted = $derived(options.some((o) => o.count !== undefined));
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
    >{#if shown}<span class="label">{shown}</span>{/if}<Icon name="chevronDown" size={18} /></button
  >
  {#if open}
    <GlassMenu {at} keep={() => el} label={menuLabel} onclose={() => (open = false)}>
      {#each options as o, i (o.id)}
        {#if o.group && o.group !== options[i - 1]?.group}<span class="group" role="presentation">{o.group}</span>{/if}
        <button
          class="option"
          role="option"
          aria-selected={ticked(o.id)}
          aria-label={o.group ? `${o.group} ${o.label.toLowerCase()}` : undefined}
          onclick={(e) => (onhold && (e.ctrlKey || e.metaKey) ? onhold(o.id) : choose(o.id))}
          use:longpress={() => onhold?.(o.id)}
          >{o.label}{#if counted}<span class="end">{#if o.count !== undefined}<span class="count">{o.count.toLocaleString()}</span>{/if}<span class="tick"><ChoiceMark kind={markKind} on={ticked(o.id)} /></span></span>{:else}<ChoiceMark kind={markKind} on={ticked(o.id)} />{/if}</button
        >
      {/each}
    </GlassMenu>
  {/if}
</span>

<style>
  /* Short of room (a breadcrumb's level: the owner, 2026-11-02, a group's picker left only its ▾), its name cut with …,
     its ▾ whole. */
  .views {
    position: relative;
    display: flex;
    min-width: 0;
  }

  .shown {
    min-width: 0;
    max-width: 100%;
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shown :global(svg) {
    flex: none;
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

  .end {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .count {
    color: var(--muted);
    font-size: 14px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .tick {
    display: flex;
    width: 18px;
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
