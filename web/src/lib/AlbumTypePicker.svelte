<script lang="ts">
  // What kind of release the Albums page shows, on the glass beside back where the Library's view picker was (the
  // owner, 2026-10-29: the views are the navigation's pages now; "albums should drop down to the big ones like EP,
  // single, and other, possibly also compilation"): only the kind shown ("All releases ▾"); a tap grows the glass
  // downwards with every kind, the one shown ticked; a choice (or a tap elsewhere, or Esc) folds it. One filter, shown
  // both ways (the owner): a choice is the Library filter's Type row, which the Filter sheet shows and the funnel
  // counts; a Type row made there shows here, or "Some types" where it is none of these.
  import type { FilterCondition } from './prefs.svelte';
  import GlassMenu from './GlassMenu.svelte';
  import Icon from './Icon.svelte';
  import { complete } from './filterRow';
  import { filterOf, setConditions } from './listFilter.svelte';

  const KIND = 'library';
  /** The kinds named on their own; Other is every other (a live album's, a soundtrack's) and the albums of none. An
   *  album has its kinds primary first ("Album", "Compilation"), so an album that is a compilation is both. */
  const NAMED = ['Album', 'EP', 'Single', 'Compilation'];
  const CHOICES = [
    { id: 'all', label: 'All releases' },
    { id: 'Album', label: 'Albums' },
    { id: 'EP', label: 'EPs' },
    { id: 'Single', label: 'Singles' },
    { id: 'Compilation', label: 'Compilations' },
    { id: 'other', label: 'Other' },
  ];
  const lower = (v: string) => v.toLowerCase();

  /** The choice the filter's Type rows are; null: rows that are none of them. */
  const chosen = $derived.by(() => {
    const on = filterOf(KIND).conditions.filter((c) => c.axis === 'type' && c.enabled !== false && complete(c));
    if (!on.length) return 'all';
    const v = on.length === 1 && on[0].value.kind === 'any_of' ? on[0].value.values.map(lower) : null;
    if (!v) return null;
    if (!on[0].not && v.length === 1) return CHOICES.find((x) => lower(x.id) === v[0])?.id ?? null;
    if (on[0].not && v.length === NAMED.length && NAMED.every((t) => v.includes(lower(t)))) return 'other';
    return null;
  });
  const shown = $derived(CHOICES.find((x) => x.id === chosen)?.label ?? 'Some types');

  let open = $state(false);
  let el = $state<HTMLElement>();
  // Under the button, in the glass menu (lib/GlassMenu).
  let at = $state<{ top: number; left: number; minWidth: number }>({ top: 0, left: 0, minWidth: 0 });
  function place() {
    const r = el?.getBoundingClientRect();
    if (r) at = { top: r.bottom + 8, left: r.left - 12, minWidth: r.width + 30 };
  }

  /** The filter's Type rows replaced by the choice's one (none for All). */
  function choose(id: string) {
    const rest = filterOf(KIND).conditions.filter((c) => c.axis !== 'type');
    const row: FilterCondition[] =
      id === 'all' ? [] : id === 'other' ? [{ axis: 'type', not: true, value: { kind: 'any_of', values: [...NAMED] } }] : [{ axis: 'type', not: false, value: { kind: 'any_of', values: [id] } }];
    setConditions(KIND, [...rest, ...row]);
    open = false;
  }
</script>

<span class="views" class:open bind:this={el}>
  <button class="shown" aria-haspopup="listbox" aria-expanded={open} aria-label={`Shows ${shown.toLowerCase()}`} onclick={() => (place(), (open = !open))}
    >{shown}<Icon name="chevronDown" size={18} /></button
  >
  {#if open}
    <GlassMenu {at} keep={() => el} label="The albums show" onclose={() => (open = false)}>
      {#each CHOICES as c (c.id)}
        <button class="option" role="option" aria-selected={c.id === chosen} onclick={() => choose(c.id)}
          >{c.label}{#if c.id === chosen}<Icon name="check" size={18} />{/if}</button
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
