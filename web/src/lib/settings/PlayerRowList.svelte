<script lang="ts">
  // The player's row (the owner, 2026-10-19): which facts show beside its info button, each with its switch; those on
  // first, in the order they show, each with a handle to drag (as My order in the output step), then the rest in the
  // overview's order. A fact a track has no value for is left out there.
  import { FACTS, type FactId } from '../facts';
  import Icon from '../Icon.svelte';
  import { prefs } from '../prefs.svelte';
  import { Reorder } from '../reorder.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';

  const label = (id: FactId) => FACTS.find((f) => f.id === id)?.label ?? id;
  const off = $derived(FACTS.filter((f) => !prefs.playerFacts.includes(f.id)).map((f) => f.id));

  function flip(id: FactId) {
    navigator.vibrate?.(8);
    prefs.playerFacts = prefs.playerFacts.includes(id) ? prefs.playerFacts.filter((x) => x !== id) : [...prefs.playerFacts, id];
  }
  let list = $state<HTMLElement>();
  const reorder = new Reorder(
    () => list,
    () => prefs.playerFacts.length,
    (from, to) => {
      const v = [...prefs.playerFacts];
      const [x] = v.splice(from, 1);
      v.splice(to, 0, x);
      prefs.playerFacts = v;
    },
  );
</script>

<ol class="facts" class:dragging={!!reorder.drag} bind:this={list} aria-label="Facts beside the info button">
  {#each prefs.playerFacts as id, i (id)}
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
