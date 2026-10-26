<script module lang="ts">
  /** One column's value in a row (decision 0042); in a list's head, its name, a tap to order by it. */
  /** `warn`: a value that is a problem (a lossy album's bitrate), in the danger colour. */
  /** A head's name that orders the list: on while it does. */
  export type HeadSort = { on: boolean; onclick: () => void };
  export type Cell = { id: string; text: string; width: number; end?: boolean; grow?: boolean; warn?: boolean; onclick?: () => void; on?: boolean };
</script>

<script lang="ts">
  // The one row (docs/ui-conventions.md → Rows): every list of things is built from it, in four sizes with one layout,
  // the title on the left and the action on the right. Large in the panel's Tracks, Info, Repair and Sync tabs (and a
  // track is a large row with its cover); medium on the pages (Playlists, Crates); small in the panel's Export tab;
  // extra large on the album lists (an album: a larger cover, and lines of what it is under its title; lib/AlbumShelf).
  // Its sibling for things shown by their picture is the square tile (lib/Tile.svelte).
  // Small and medium are Finamp's add-to-playlist tiles (tinted in the accent, a divider before the action); large is
  // Finamp's list tile (plain, a cover or control in front).
  //
  // Parts, left to right: `front` (a control of its own before the tap target: an album's cover that plays), `lead`
  // (a cover, a checkbox), the title with its `badges` after it (and `sub`, a second line,
  // under it), at the end of the text its `count` (a number, a duration: muted) and `meta` (its flags), then `action`. The text (with the lead) is the row's
  // own tap target when `onclick` is given, a label with `as="label"` (its checkbox toggles).
  // `cells`: the list's columns where there is room (decision 0042, lib/columns), between the text and its end; in a
  // list's `head` row they are its column names, a tap orders the list by one.
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';
  import { COLUMN_GAP } from './columns.svelte';

  let {
    size,
    title,
    sub,
    lead,
    front,
    badges,
    count,
    meta,
    action,
    onclick,
    as,
    disabled = false,
    label,
    tooltip,
    active = false,
    strong = false,
    off = false,
    wrap = false,
    class: extra = '',
    style,
    attach,
    attachMain,
    cells,
    head = false,
    sorts,
  }: {
    size: 'small' | 'medium' | 'large' | 'xl';
    title: string;
    /** A second line: muted (large), or under the title on small and medium; on extra large, lines of its own. */
    sub?: string | Snippet;
    lead?: Snippet;
    /** Before the tap target, a control of its own (an album's cover that plays it). */
    front?: Snippet;
    badges?: Snippet;
    count?: string | number;
    meta?: Snippet;
    action?: Snippet;
    onclick?: () => void;
    /** What the text part is: a button when `onclick` is given, else plain; a label wraps a lead checkbox. */
    as?: 'button' | 'div' | 'label';
    disabled?: boolean;
    /** The tap target's accessible name, when the title alone does not say what a tap does. */
    label?: string;
    tooltip?: string;
    /** Viewed in the panel, or ticked: a border. */
    active?: boolean;
    /** Holding what matters now (the playing track is in this playlist): a stronger tint. */
    strong?: boolean;
    /** Not usable now (a missing file): dimmed. */
    off?: boolean;
    /** The action may go under the text when the row is narrow (a large row's controls). */
    wrap?: boolean;
    class?: string;
    style?: string;
    /** On the whole row (a long-press, a swipe). */
    attach?: Attachment<HTMLElement>;
    /** On the text part (a long-press on a track's row). */
    attachMain?: Attachment<HTMLElement>;
    /** The list's columns shown (decision 0042). */
    cells?: readonly Cell[];
    /** The list's head: its column names over its rows, muted. */
    head?: boolean;
    /** In a head, the title's and the count's names as columns that order the list too (the owner, 2026-10-26: the
     *  leftmost column sorts as the others do). */
    sorts?: { title?: HeadSort; count?: HeadSort };
  } = $props();

  const tag = $derived(as ?? (onclick ? 'button' : 'div'));
  // A name (an album) grows with the title into the room the columns leave; a number or a date keeps its width. The
  // columns together start from their widths, never from what a row's text holds, so they line up from row to row.
  const basis = (c: Cell) => (c.grow ? `1 1 ${c.width}px` : `0 0 ${c.width}px`);
  const cellsFlex = $derived(
    cells?.length
      ? `${cells.filter((c) => c.grow).length} 1 ${cells.reduce((w, c) => w + c.width, 0) + COLUMN_GAP * (cells.length - 1)}px`
      : undefined,
  );
  const plain = $derived(size === 'large' || size === 'xl');
  const tinted = $derived(!plain);
</script>

<div
  class="row {size} {extra}"
  class:large={plain}
  class:tinted
  class:active
  class:strong
  class:off
  class:wrap
  class:led={!!lead || !!front}
  class:has-cells={!!cells?.length}
  class:head
  class:press={tinted && !!onclick && !disabled}
  title={tooltip}
  {style}
  {@attach attach}
>
  {#if front}{@render front()}{/if}
  {#if tag === 'button'}
    <button type="button" class="main" {disabled} aria-label={label} {onclick} {@attach attachMain}>{@render inside()}</button>
  {:else if tag === 'label'}
    <label class="main" aria-label={label} {@attach attachMain}>{@render inside()}</label>
  {:else}
    <div class="main" aria-label={label} {@attach attachMain}>{@render inside()}</div>
  {/if}
  {#if action}
    {#if tinted}<span class="sep" aria-hidden="true"></span>{/if}
    <span class="act">{@render action()}</span>
  {/if}
</div>

{#snippet inside()}
  {#if lead}<span class="lead">{@render lead()}</span>{/if}
  <span class="text">
    <span class="line"
      >{#if head && sorts?.title}<button type="button" class="title cell" class:on={sorts.title.on} onclick={sorts.title.onclick}>{title}</button
        >{:else}<span class="title">{title}</span>{/if}{@render badges?.()}</span
    >
    {#if typeof sub === 'string'}<span class="sub">{sub}</span>{:else if sub}<span class="sub">{@render sub()}</span>{/if}
  </span>
  {#if cells?.length}
    <span class="cells" style:flex={cellsFlex} style:gap="{COLUMN_GAP}px">
      {#each cells as c (c.id)}
        {#if c.onclick}
          <button type="button" class="cell" class:end={c.end} class:on={c.on} style:flex={basis(c)} onclick={c.onclick}>{c.text}</button>
        {:else}
          <span class="cell" class:end={c.end} class:warn={c.warn} style:flex={basis(c)}>{c.text}</span>
        {/if}
      {/each}
    </span>
  {/if}
  {#if count !== undefined || meta}<span class="meta"
      >{#if count !== undefined}{#if head && sorts?.count}<button type="button" class="n cell end" class:on={sorts.count.on} onclick={sorts.count.onclick}>{count}</button
          >{:else}<span class="n">{count}</span>{/if}{/if}{@render meta?.()}</span
    >{/if}
{/snippet}

<style>
  /* The columns (decision 0042): each from its width, so they line up from row to row; the room left over goes to the
     title (twice a column's share, from the title's own room in lib/columns: 280 px) and the names among them. The
     row's end keeps a fixed width with them, a time or a menu or none. */
  .has-cells .text {
    flex: 2 1 280px;
  }

  .cells {
    min-width: 0;
    display: flex;
  }

  .cell {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.88rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .cell.end {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .cell.warn {
    color: var(--danger-ink);
  }

  /* A head's names: those that order the list are buttons, the rest the same words plain. */
  .head .cell {
    font-size: 0.8rem;
    font-weight: 600;
  }

  button.cell {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  button.cell.end {
    text-align: right;
  }

  button.cell:hover,
  button.cell.on {
    color: var(--text);
  }

  .has-cells .meta {
    min-width: 52px;
    justify-content: flex-end;
  }

  .has-cells .act {
    min-width: 40px;
  }

  .large.head {
    min-height: 32px;
  }

  .head .title,
  .head .meta {
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .row {
    /* Over anything behind it (a track's swipe shows what it does there). */
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    box-sizing: border-box;
    color: var(--text);
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: inherit;
    align-self: stretch;
    padding: 0;
    border: 0;
    border-radius: inherit;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }

  button.main,
  label.main {
    cursor: pointer;
  }

  button.main:disabled {
    cursor: default;
  }

  .lead {
    flex: none;
    display: flex;
    align-items: center;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }

  .sub {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }

  .meta {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    font-variant-numeric: tabular-nums;
  }

  .act {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* The action's usual kinds, as the consumers' buttons: an icon in the accent, or a quiet text button. */
  .act :global(button.icon) {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    cursor: pointer;
  }

  .act :global(button.text) {
    min-height: 36px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: inherit;
    font: inherit;
    font-size: 0.9rem;
    white-space: nowrap;
    cursor: pointer;
  }

  .act :global(button:disabled) {
    opacity: 0.5;
    cursor: default;
  }

  .sep {
    flex: none;
    width: 1.5px;
    height: 24px;
    background: color-mix(in srgb, var(--text) 35%, transparent);
  }

  /* Its contents only: the row stays opaque over what is behind it. */
  .row.off > * {
    opacity: 0.5;
  }

  /* Small and medium: Finamp's add-to-playlist tile, tinted in the accent (stronger when it holds what plays). */
  .tinted {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    transition:
      background 120ms,
      transform 80ms;
  }

  .tinted.strong {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .press:active {
    transform: scale(0.97);
  }

  /* Viewed in the panel, or ticked: a border only, so it reads apart from the tint. */
  .active {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  /* The keyboard's ring is the row's, as the viewed one's (the owner, 2026-10-23: the same size), not the browser's
     round the tap target only; in the text colour, so the two read apart. */
  .main:focus-visible {
    outline: none;
  }

  .row:has(> .main:focus-visible) {
    box-shadow: inset 0 0 0 2px var(--text);
  }

  .tinted .n {
    font-size: 0.8rem;
  }

  .tinted .n,
  .tinted .sub {
    opacity: 0.65;
    color: inherit;
  }

  .tinted .sub {
    font-size: 0.8rem;
  }

  .medium {
    min-height: 48px;
    padding: 6px 8px 6px 12px;
    border-radius: 12px;
  }

  .medium .act :global(button.icon) {
    width: 26px;
    height: 26px;
  }

  .small {
    min-height: 40px;
    padding: 2px 4px 2px 12px;
    border-radius: 10px;
    font-size: 0.95rem;
  }

  .small .title {
    -webkit-line-clamp: 1;
    line-clamp: 1;
  }

  .small .act :global(button.icon) {
    width: 36px;
    height: 36px;
    color: inherit;
    opacity: 0.7;
  }

  /* Large: Finamp's list tile, plain (a track's row is opaque over what its swipe shows: --row-bg), 64 px, with a
     60 px cover or a control in front; two lines of title, a muted line under it. */
  .large {
    min-height: 64px;
    padding: 6px 4px 6px 12px;
    border-radius: 8px;
    background: var(--row-bg, transparent);
    gap: 4px;
  }

  .large .main {
    gap: 10px;
  }

  /* A cover in front keeps a gutter from the row's edge, inside the border of a viewed row (the owner, 2026-10-23). */
  .large.led {
    padding: 2px 4px 2px 6px;
  }

  /* The action's buttons are as tall as the row (a track's heart). */
  .large .act {
    align-self: stretch;
  }

  .large.wrap .act {
    align-self: center;
  }

  .large.wrap {
    flex-wrap: wrap;
    row-gap: 4px;
  }

  /* Its controls give way to the width: they go under the text, then under each other, never wider than the row. */
  .large.wrap .act {
    flex: 0 1 auto;
    flex-wrap: wrap;
    justify-content: flex-end;
    max-width: 100%;
    min-width: 0;
    margin-left: auto;
  }

  .large.wrap .act > :global(*) {
    max-width: 100%;
    min-width: 0;
  }

  /* Under a pointer the whole row lights, its action included; not on touch, where a hover stays after the tap. */
  @media (hover: hover) {
    .large:has(> button.main:hover:not(:disabled)),
    .large:has(> label.main:hover) {
      background: color-mix(in srgb, var(--text) 7%, var(--row-bg, transparent));
    }
  }

  .large .title {
    font-size: 15.5px;
    font-weight: 500;
    line-height: 1.1;
  }

  .large .sub {
    margin-top: 2px;
    font-size: 13px;
  }

  .large .n {
    color: var(--muted);
    font-size: 13px;
  }

  .large .act :global(button.icon) {
    width: 44px;
    height: 44px;
  }

  /* Extra large: a large row with more to say, an album's: its lines wrap and keep their own colours. */
  /* Room all round, so a cover stays inside the border of a viewed row. */
  .xl,
  .xl.led {
    min-height: 84px;
    gap: 12px;
    padding: 6px;
    border-radius: 12px;
  }

  .xl .main {
    gap: 12px;
  }

  .xl .title {
    font-size: 16px;
    font-weight: 600;
  }

  .xl .sub {
    white-space: normal;
    color: inherit;
    font-size: inherit;
  }
</style>
