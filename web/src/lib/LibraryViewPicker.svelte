<script lang="ts">
  // What the Library shows, on the glass beside back (the owner, 2026-10-25): only the view shown ("Albums ▾"); a tap
  // grows the glass downwards with every view, the one shown ticked; a choice (or a tap elsewhere, or Esc) folds it.
  import GlassMenu from './GlassMenu.svelte';
  import Icon from './Icon.svelte';
  import { prefs } from './prefs.svelte';

  import { LIBRARY_VIEWS as VIEWS } from './libraryViews';
  let open = $state(false);
  let el = $state<HTMLElement>();
  // Under the button, in the glass menu (lib/GlassMenu).
  let at = $state<{ top: number; left: number; minWidth: number }>({ top: 0, left: 0, minWidth: 0 });
  function place() {
    const r = el?.getBoundingClientRect();
    if (r) at = { top: r.bottom + 8, left: r.left - 12, minWidth: r.width + 30 };
  }
  const shown = $derived(VIEWS.find((v) => v.value === prefs.libraryView) ?? VIEWS[0]);

  function choose(v: (typeof VIEWS)[number]['value']) {
    prefs.libraryView = v;
    open = false;
  }
</script>

<span class="views" class:open bind:this={el}>
  <button class="shown" aria-haspopup="listbox" aria-expanded={open} aria-label={`Shows ${shown.label.toLowerCase()}`} onclick={() => (place(), (open = !open))}
    >{shown.label}<Icon name="chevronDown" size={18} /></button
  >
  {#if open}
    <GlassMenu {at} keep={() => el} label="The library shows" onclose={() => (open = false)}>
      {#each VIEWS as v (v.value)}
        <button class="option" role="option" aria-selected={v.value === prefs.libraryView} onclick={() => choose(v.value)}
          >{v.label}{#if v.value === prefs.libraryView}<Icon name="check" size={18} />{/if}</button
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
</style>
