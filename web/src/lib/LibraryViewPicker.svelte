<script lang="ts">
  // What the Library shows, on the glass beside back (the owner, 2026-10-25): only the view shown ("Albums ▾"); a tap
  // grows the glass downwards with every view, the one shown ticked; a choice (or a tap elsewhere, or Esc) folds it.
  import Icon from './Icon.svelte';
  import { portal } from './portal';
  import { prefs } from './prefs.svelte';

  const VIEWS: { value: 'albums' | 'artists' | 'tracks'; label: string }[] = [
    { value: 'albums', label: 'Albums' },
    { value: 'artists', label: 'Artists' },
    { value: 'tracks', label: 'Tracks' },
  ];
  let open = $state(false);
  let el = $state<HTMLElement>();
  let list = $state<HTMLElement>();
  // Drawn at the page's end, under the button: inside the title's glass its own blur would see only that glass (a
  // backdrop inside a backdrop), not the page behind.
  let at = $state({ top: 0, left: 0, width: 0 });
  function place() {
    const r = el?.getBoundingClientRect();
    if (r) at = { top: r.bottom + 8, left: r.left - 12, width: r.width + 30 };
  }
  const shown = $derived(VIEWS.find((v) => v.value === prefs.libraryView) ?? VIEWS[0]);

  $effect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!el?.contains(e.target as Node) && !list?.contains(e.target as Node)) open = false;
    };
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (open = false);
    addEventListener('pointerdown', away, true);
    addEventListener('keydown', esc);
    return () => {
      removeEventListener('pointerdown', away, true);
      removeEventListener('keydown', esc);
    };
  });
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
    <span class="list" role="listbox" aria-label="The library shows" use:portal bind:this={list} style:top="{at.top}px" style:left="{at.left}px" style:min-width="{at.width}px">
      {#each VIEWS as v (v.value)}
        <button role="option" aria-selected={v.value === prefs.libraryView} onclick={() => choose(v.value)}
          >{v.label}{#if v.value === prefs.libraryView}<Icon name="check" size={18} />{/if}</button
        >
      {/each}
    </span>
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

  /* The glass grown downwards: the same thin glass as the title's, under it. */
  .list {
    position: fixed;
    z-index: 23;
    display: grid;
    padding: 6px;
    border-radius: 18px;
    background: color-mix(in srgb, var(--float-bg) 45%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(1.3);
    backdrop-filter: blur(10px) saturate(1.3);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.18);
  }

  .list button {
    min-height: 44px;
    padding: 0 12px;
    font-size: 16px;
    font-weight: 500;
  }

  .list button[aria-selected='true'] {
    color: var(--accent-ink);
    font-weight: 600;
  }

  @media (hover: hover) {
    .list button:hover {
      background: color-mix(in srgb, var(--text) 8%, transparent);
    }
  }
</style>
