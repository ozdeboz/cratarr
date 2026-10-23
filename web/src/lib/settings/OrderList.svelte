<script lang="ts" generics="T extends string">
  // Things switched on in the user's order (the owner: the player's row's facts, 2026-10-19; a list's profile rows,
  // 2026-10-23): those on first, in their order, each with a handle to drag (as My order in the output step), then the
  // rest in the order offered, each with its switch.
  import Icon from '../Icon.svelte';
  import { Reorder } from '../reorder.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';

  let {
    items,
    on,
    label: name,
    onchange,
  }: {
    /** All of them, in the order offered. */
    items: readonly { id: T; label: string }[];
    /** Those on, in their order. */
    on: readonly T[];
    /** The list's accessible name. */
    label: string;
    onchange: (on: T[]) => void;
  } = $props();

  const label = (id: T) => items.find((f) => f.id === id)?.label ?? id;
  const off = $derived(items.filter((f) => !on.includes(f.id)).map((f) => f.id));

  function flip(id: T) {
    navigator.vibrate?.(8);
    onchange(on.includes(id) ? on.filter((x) => x !== id) : [...on, id]);
  }
  let list = $state<HTMLElement>();
  const reorder = new Reorder(
    () => list,
    () => on.length,
    (from, to) => {
      const v = [...on];
      const [x] = v.splice(from, 1);
      v.splice(to, 0, x);
      onchange(v);
    },
  );
</script>

<ol class="facts" class:dragging={!!reorder.drag} bind:this={list} aria-label={name}>
  {#each on as id, i (id)}
    <li class:lifted={reorder.drag?.from === i} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
      <button class="row" role="switch" aria-checked="true" onclick={() => flip(id)}><span>{label(id)}</span><SwitchTrack on={true} /></button>
      <button
        class="grip"
        aria-label={`Drag ${label(id)} to reorder`}
        onpointerdown={(e) => reorder.down(e, i)}
        onpointermove={reorder.move}
        onpointerup={reorder.end}
        onpointercancel={reorder.end}><Icon name="order" size={22} /></button
      >
    </li>
  {/each}
  {#each off as id (id)}
    <li>
      <button class="row" role="switch" aria-checked="false" onclick={() => flip(id)}><span>{label(id)}</span><SwitchTrack on={false} /></button>
      <span class="grip-room" aria-hidden="true"></span>
    </li>
  {/each}
</ol>

<style>
  .facts {
    position: relative;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .facts li {
    position: relative;
    display: flex;
    align-items: center;
    background: var(--surface);
  }

  .facts.dragging li {
    transition: transform 200ms ease;
  }

  .facts.dragging li.lifted {
    z-index: 1;
    transition: none;
  }

  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 48px;
    padding: 0 8px 0 16px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .grip,
  .grip-room {
    flex: none;
    width: 44px;
    height: 48px;
  }

  .grip {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    touch-action: none;
    cursor: grab;
  }
</style>
