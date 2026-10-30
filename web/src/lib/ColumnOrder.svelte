<script lang="ts">
  // View's columns (decision 0042): a list's columns in the owner's order, dragged by their handle (or moved with the
  // arrow keys on it) into the order wanted; the list shows them from the top as its width allows. The ones it has no
  // room for now are dimmed. Only the columns the list has values for are listed.
  import { ordered, setOrder, type ColumnKind } from './columns.svelte';
  import Card from './Card.svelte';
  import Icon from './Icon.svelte';
  import { Reorder } from './reorder.svelte';

  let { kind, shown, offered }: { kind: ColumnKind; shown: readonly string[]; offered: readonly string[] } = $props();

  const items = $derived(ordered(kind).filter((c) => offered.includes(c.id)));
  let listEl = $state<HTMLElement>();

  function move(from: number, to: number) {
    const ids = items.map((c) => c.id);
    const [id] = ids.splice(from, 1);
    ids.splice(to, 0, id);
    setOrder(kind, [...ids, ...ordered(kind).flatMap((c) => (ids.includes(c.id) ? [] : [c.id]))]);
  }

  const reorder = new Reorder(
    () => listEl,
    () => items.length,
    move,
  );

  function key(e: KeyboardEvent, i: number) {
    const to = e.key === 'ArrowUp' ? i - 1 : e.key === 'ArrowDown' ? i + 1 : null;
    if (to === null || to < 0 || to >= items.length) return;
    e.preventDefault();
    move(i, to);
  }
</script>

<Card title="Columns" label="Columns">
  <ol class="cols" class:dragging={!!reorder.drag} bind:this={listEl}>
    {#each items as c, i (c.id)}
      <li class:lifted={reorder.drag?.from === i} class:off={!shown.includes(c.id)} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
        <span class="name">{c.label}</span>
        <button
          class="grip"
          aria-label={`Move ${c.label}`}
          onpointerdown={(e) => reorder.down(e, i)}
          onpointermove={reorder.move}
          onpointerup={reorder.end}
          onpointercancel={reorder.end}
          onkeydown={(e) => key(e, i)}><Icon name="order" size={22} /></button
        >
      </li>
    {/each}
  </ol>
</Card>

<style>
  .cols {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* As View's other choices (lib/ChoiceList): a line each. */
  li {
    position: relative;
    display: flex;
    align-items: center;
    min-height: 44px;
    border-radius: 8px;
    font-size: 15px;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  /* No room for it in the list now. */
  li.off .name {
    color: var(--muted);
  }

  .grip {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-right: -10px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    touch-action: none;
    cursor: grab;
  }

  .dragging li {
    transition: transform 200ms ease;
  }

  /* The dragged one: opaque over the ones it passes. */
  .dragging li.lifted {
    z-index: 1;
    background: color-mix(in srgb, var(--text) 10%, var(--surface));
    transition: none;
  }

  .dragging li.lifted .grip {
    cursor: grabbing;
  }
</style>
