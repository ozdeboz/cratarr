<script lang="ts" generics="T extends string">
  // Things switched on in the user's order (the owner: the player's row's facts, 2026-10-19; a list's profile rows,
  // 2026-10-23): those on first, in their order, each with a handle to drag (as My order in the output step), then the
  // rest in the order offered, each with its switch. With `fold`, a fold line among those on, dragged as they are: what
  // is above it stays when the thing is folded (a list's profile, the owner 2026-10-23), what is below folds away.
  import Icon from '../Icon.svelte';
  import { Reorder } from '../reorder.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';

  let {
    items,
    on,
    label: name,
    onchange,
    fold = null,
  }: {
    /** All of them, in the order offered. */
    items: readonly { id: T; label: string }[];
    /** Those on, in their order. */
    on: readonly T[];
    /** The list's accessible name. */
    label: string;
    onchange: (on: T[]) => void;
    /** The fold line: how many of those on are above it. */
    fold?: { at: number; onchange: (at: number) => void } | null;
  } = $props();

  /** Those on, the fold line among them (null). */
  const entries = $derived<(T | null)[]>(fold ? [...on.slice(0, fold.at), null, ...on.slice(fold.at)] : [...on]);

  const label = (id: T) => items.find((f) => f.id === id)?.label ?? id;
  const off = $derived(items.filter((f) => !on.includes(f.id)).map((f) => f.id));

  function flip(id: T) {
    navigator.vibrate?.(8);
    // One switched off above the fold line takes the line up with it; one switched on comes in last, below it.
    if (fold && on.indexOf(id) >= 0 && on.indexOf(id) < fold.at) fold.onchange(fold.at - 1);
    onchange(on.includes(id) ? on.filter((x) => x !== id) : [...on, id]);
  }
  let list = $state<HTMLElement>();
  const reorder = new Reorder(
    () => list,
    () => entries.length,
    (from, to) => {
      const v = [...entries];
      const [x] = v.splice(from, 1);
      v.splice(to, 0, x);
      onchange(v.filter((e): e is T => e !== null));
      if (fold) fold.onchange(v.indexOf(null));
    },
  );
</script>

<ol class="facts" class:dragging={!!reorder.drag} bind:this={list} aria-label={name}>
  {#each entries as id, i (id ?? '|')}
    <li class:lifted={reorder.drag?.from === i} class:line={id === null} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
      {#if id === null}
        <!-- The fold line, as the profile draws it (lib/ListProfile): above it stays, below it folds. -->
        <span class="row fold"><span class="pill"><Icon name="chevronUp" size={14} />Fold</span></span>
      {:else}
        <button class="row" role="switch" aria-checked="true" onclick={() => flip(id)}><span>{label(id)}</span><SwitchTrack on={true} /></button>
      {/if}
      <button
        class="grip"
        aria-label={id === null ? 'Drag the fold line' : `Drag ${label(id)} to reorder`}
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

  /* The fold line: a rule with its pill on it. */
  .row.fold {
    position: relative;
    justify-content: center;
    cursor: default;
  }

  .row.fold::before {
    content: '';
    position: absolute;
    top: 50%;
    right: 8px;
    left: 16px;
    border-top: 1px dashed var(--border);
  }

  .pill {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 12px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
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
