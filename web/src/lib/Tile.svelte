<script lang="ts">
  // The square tile (docs/ui-conventions.md → Tiles): a thing shown by its picture, in a grid of columns as wide as
  // the page's Size makes them (an album's cover). Row's sibling: the picture on top, filling the tile's width, the
  // title and `sub` under it, the `action` (Play) over the picture's lower right corner and the `tag` (what the list is
  // ordered by) over its upper left one. The picture and the text are the
  // tile's tap target when `onclick` is given.
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';

  let {
    title,
    dot = null,
    sub,
    image,
    action,
    over,
    tag = null,
    onclick,
    label,
    active = false,
    strong = false,
    attach,
  }: {
    title: string;
    /** Its library, as a dot in that library's colour before the title (decision 0034). */
    dot?: { colour: string; name: string } | null;
    /** Under the title: muted lines. */
    sub?: string | Snippet;
    /** The square picture; it fills the tile's width. */
    image: Snippet;
    /** Over the picture's lower right corner. */
    action?: Snippet;
    /** Over the picture, as large as it: what sits on it (an album's Play in its upper right corner, lib/CoverPlay; the
     *  wait of its opening, lib/CoverWait). */
    over?: Snippet;
    /** A pill over the picture's upper left corner: the value the list is ordered by (a year, a date). */
    tag?: string | null;
    onclick?: () => void;
    /** The tap target's accessible name, when the title alone does not say what a tap does. */
    label?: string;
    /** Viewed in the panel: a border. */
    active?: boolean;
    /** Holding what matters now (it plays): tinted. */
    strong?: boolean;
    /** On the whole tile (a long-press). */
    attach?: Attachment<HTMLElement>;
  } = $props();
</script>

<div class="tile" class:active class:strong {@attach attach}>
  {#if onclick}
    <button type="button" class="main" aria-label={label} {onclick}>{@render inside()}</button>
  {:else}
    <div class="main">{@render inside()}</div>
  {/if}
  {#if over}<span class="over">{@render over()}</span>{/if}
  {#if tag}<span class="tag">{tag}</span>{/if}
  {#if action}<span class="act">{@render action()}</span>{/if}
</div>

{#snippet inside()}
  <span class="image">{@render image()}</span>
  <span class="title">{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{title}</span>
  {#if typeof sub === 'string'}<span class="sub">{sub}</span>{:else if sub}<span class="sub">{@render sub()}</span>{/if}
{/snippet}

<style>
  /* Its own width is the picture's: the action sits on the picture's corner by it (cqw). */
  .tile {
    position: relative;
    container-type: inline-size;
    padding: 6px;
    border-radius: 12px;
    color: var(--text);
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }

  button.main {
    cursor: pointer;
  }

  .image {
    display: block;
    aspect-ratio: 1;
    margin-bottom: 6px;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface);
  }

  .image > :global(*) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 600;
  }

  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 50%;
    vertical-align: 2px;
  }

  .sub {
    display: block;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
  }

  /* The ordered-by pill, 8 px in from the picture's upper left corner, on the floating controls' surface (app.css). */
  .tag {
    position: absolute;
    top: 14px;
    left: 14px;
    max-width: calc(100% - 28px);
    overflow: hidden;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
    pointer-events: none;
  }

  /* As large as the picture, over it; only its buttons take a tap (the wait, while on, takes them all). */
  .over {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 100cqw;
    height: 100cqw;
    border-radius: 8px;
    pointer-events: none;
  }

  .over > :global(.cover-play) {
    pointer-events: auto;
  }

  /* 8 px in from the picture's corner: the padding (6 px) + its width (100cqw, the content box) − the button − 8 px. */
  .act {
    position: absolute;
    top: calc(100cqw - 46px);
    right: 14px;
  }

  /* The action's usual kind: a round button in the accent over the picture. */
  .act :global(button.icon) {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.35);
    cursor: pointer;
  }

  .act :global(button:disabled) {
    opacity: 0.6;
    cursor: default;
  }

  .main:hover .image,
  .main:focus-visible .image {
    filter: brightness(1.08);
  }

  .strong {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .active {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
</style>
