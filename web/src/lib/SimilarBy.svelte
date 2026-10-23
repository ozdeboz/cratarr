<script lang="ts">
  // "Similar by" (the owner, 2026-10-23): what makes music alike for Radio and Autoplay, a row that opens to the list
  // of them. Each is switched on or off by its mark and dragged by its handle (or moved with the arrow keys on it) into
  // the order wanted: the first counts most, the next half as much, and so on. Until one is touched they all count the
  // same. Kept with the account's settings; the server reads them for each choice.
  import Icon from './Icon.svelte';
  import { SIGNALS, setSimilarBy, similar, type Signal } from './mySettings.svelte';
  import { Reorder } from './reorder.svelte';

  let open = $state(false);
  let listEl = $state<HTMLElement>();

  const equal = $derived(similar.by.length === 0);
  /** The ones on in their order, then the ones off as offered; all on while none is chosen. */
  const items = $derived(equal ? SIGNALS.map((s) => ({ ...s, on: true })) : [
    ...similar.by.map((id) => ({ ...SIGNALS.find((s) => s.id === id)!, on: true })),
    ...SIGNALS.filter((s) => !similar.by.includes(s.id)).map((s) => ({ ...s, on: false })),
  ]);
  const summary = $derived(equal ? 'All, equally' : similar.by.map((id) => SIGNALS.find((s) => s.id === id)!.label).join(', '));

  /** The order kept: the ones on, as listed. */
  const save = (list: { id: Signal; on: boolean }[]) => setSimilarBy(list.filter((s) => s.on).map((s) => s.id));

  function move(from: number, to: number) {
    const list = [...items];
    const [x] = list.splice(from, 1);
    list.splice(to, 0, x);
    save(list);
  }

  function toggle(i: number) {
    // Never none: the last one on stays.
    if (items[i].on && items.filter((s) => s.on).length === 1) return;
    save(items.map((s, j) => (j === i ? { ...s, on: !s.on } : s)));
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

<div class="similar">
  <button class="row" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="label">Similar by</span>
    <span class="summary">{summary}</span>
    <Icon name={open ? 'chevronUp' : 'chevronDown'} size={20} />
  </button>
  {#if open}
    <ol class="signals" class:dragging={!!reorder.drag} bind:this={listEl} aria-label="Similar by">
      {#each items as s, i (s.id)}
        <li class:lifted={reorder.drag?.from === i} class:off={!s.on} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
          <button class="mark" role="switch" aria-checked={s.on} aria-label={s.label} onclick={() => toggle(i)}><Icon name={s.on ? 'circleCheck' : 'circle'} size={22} /></button>
          <span class="name">{s.label}</span>
          <button
            class="grip"
            aria-label={`Move ${s.label}`}
            onpointerdown={(e) => reorder.down(e, i)}
            onpointermove={reorder.move}
            onpointerup={reorder.end}
            onpointercancel={reorder.end}
            onkeydown={(e) => key(e, i)}><Icon name="order" size={22} /></button
          >
        </li>
      {/each}
    </ol>
    <div class="foot">
      <small>{equal ? 'All count the same.' : 'The first counts most.'}</small>
      {#if !equal}<button class="reset" onclick={() => setSimilarBy([])}>All the same</button>{/if}
    </div>
  {/if}
</div>

<style>
  .similar {
    margin-top: 4px;
    border-top: 1px solid var(--border);
  }

  /* As lib/SettingRow: a line, its name first and what is set after it. */
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  .label {
    flex: none;
  }

  .summary {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row :global(svg) {
    flex: none;
    color: var(--muted);
  }

  /* As lib/ColumnOrder: a line each, its handle at the end. */
  .signals {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    border-radius: 8px;
    font-size: 15px;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  li.off .name {
    color: var(--muted);
  }

  .mark {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  li:not(.off) .mark :global(svg) {
    color: var(--art-accent, var(--accent));
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

  .dragging li.lifted {
    z-index: 1;
    background: color-mix(in srgb, var(--text) 10%, var(--surface));
    transition: none;
  }

  .dragging li.lifted .grip {
    cursor: grabbing;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 40px;
    color: var(--muted);
  }

  .reset {
    padding: 6px 0;
    border: 0;
    background: none;
    color: var(--art-accent, var(--accent));
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
