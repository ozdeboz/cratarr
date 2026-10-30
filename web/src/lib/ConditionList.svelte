<script lang="ts">
  // A crate's or a filter's conditions as cards (lib/ConditionCard, decision 0061), and "+ Condition" choosing an axis
  // for a new one, which opens unfolded. The crate editor (lib/CrateEditor) and the Filter sheet (lib/FilterSheet) both
  // draw it; the axes and their counts come from them.
  import ConditionCard, { type ListAxis } from './ConditionCard.svelte';
  import type { Condition } from './api';
  import Icon from './Icon.svelte';
  import type { FilterCondition } from './prefs.svelte';

  let {
    conditions = $bindable(),
    axes,
    items = null,
    bpms = null,
    crate = false,
  }: {
    conditions: FilterCondition[];
    /** null: not read yet (the cards then show no name rather than the axis's key first). */
    axes: ListAxis[] | null;
    /** What the list's items are when they hold tracks: any or all of each one's tracks (lib/ConditionCard). */
    items?: string | null;
    bpms?: readonly (number | null | undefined)[] | null;
    /** A crate's conditions: a Sounds like's curve counts among the tracks its others leave (0.11, lib/ConditionCard
     *  `within`); a filter's count the whole library, the list it is on narrowing it besides. */
    crate?: boolean;
  } = $props();

  // One card open at a time keeps the list short; a new one opens.
  let opened = $state<number | null>(null);
  let adding = $state(false);
  function add(a: ListAxis) {
    const value: Condition['value'] = a.kind === 'yes_no' ? { kind: 'yes' } : a.kind === 'range' ? { kind: 'range', min: null, max: null } : { kind: 'any_of', values: [] };
    conditions = [...conditions, { axis: a.key, not: false, value }];
    opened = conditions.length - 1;
    adding = false;
  }
  function remove(i: number) {
    conditions = conditions.filter((_, j) => j !== i);
    opened = null;
  }
</script>

<ol class="list">
  {#each conditions as _, i (i)}
    <ConditionCard
      condition={conditions[i]}
      onchange={(next) => (conditions = conditions.map((c, j) => (j === i ? next : c)))}
      axis={axes?.find((a) => a.key === conditions[i].axis)}
      known={axes !== null}
      {items}
      {bpms}
      within={crate ? conditions.filter((_, j) => j !== i) : null}
      bind:open={() => opened === i, (o) => (opened = o ? i : null)}
      onremove={() => remove(i)}
    />
  {/each}
</ol>
{#if adding}
  <div class="axes" role="group" aria-label="Add a condition on">
    <!-- Sounds like is seeded from a track or an album (its menu), not added here. -->
    {#each (axes ?? []).filter((a) => a.kind !== 'like') as a (a.key)}<button class="axis" onclick={() => add(a)}>{a.label}</button>{/each}
    <button class="axis" aria-label="Close" onclick={() => (adding = false)}><Icon name="close" size={16} /></button>
  </div>
{:else}
  <button class="add" onclick={() => (adding = true)} disabled={!axes?.length}><Icon name="plus" size={18} /><span>Condition</span></button>
{/if}

<style>
  /* Cards as wide as the sheet, never as their contents (a long summary is cut short instead). */
  .list {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    margin: 0;
    padding: 0;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    margin-top: 6px;
    padding: 10px 12px;
    border: 1px dashed var(--border);
    border-radius: 14px;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    cursor: pointer;
  }

  .axes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }

  .axis {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
