<script lang="ts">
  // The one row (docs/ui-conventions.md → Rows): every list of things is built from it, in four sizes with one layout,
  // the title on the left and the action on the right. Large in the panel's Tracks, Info, Repair and Sync tabs (and a
  // track is a large row with its cover); medium on the pages (Playlists, Crates); small in the panel's Export tab;
  // extra large on the album lists (an album: a larger cover, and lines of what it is under its title; lib/AlbumShelf).
  // Its sibling for things shown by their picture is the square tile (lib/Tile.svelte).
  // Small and medium are Finamp's add-to-playlist tiles (tinted in the accent, a divider before the action); large is
  // Finamp's list tile (plain, a cover or control in front).
  //
  // Parts, left to right: `lead` (a cover, a checkbox), the title with its `badges` after it (and `sub`, a second line,
  // under it), at the end of the text its `count` (a number, a duration: muted) and `meta` (its flags), then `action`. The text (with the lead) is the row's
  // own tap target when `onclick` is given, a label with `as="label"` (its checkbox toggles).
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';

  let {
    size,
    title,
    sub,
    lead,
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
  }: {
    size: 'small' | 'medium' | 'large' | 'xl';
    title: string;
    /** A second line: muted (large), or under the title on small and medium; on extra large, lines of its own. */
    sub?: string | Snippet;
    lead?: Snippet;
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
  } = $props();

  const tag = $derived(as ?? (onclick ? 'button' : 'div'));
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
  class:led={!!lead}
  class:press={tinted && !!onclick && !disabled}
  title={tooltip}
  {style}
  {@attach attach}
>
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
    <span class="line"><span class="title">{title}</span>{@render badges?.()}</span>
    {#if typeof sub === 'string'}<span class="sub">{sub}</span>{:else if sub}<span class="sub">{@render sub()}</span>{/if}
  </span>
  {#if count !== undefined || meta}<span class="meta">{#if count !== undefined}<span class="n">{count}</span>{/if}{@render meta?.()}</span>{/if}
{/snippet}

<style>
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
    color: var(--accent);
    cursor: pointer;
  }

  .act :global(button.text) {
    min-height: 36px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
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

  .large.led {
    padding: 2px 4px 2px 0;
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

  .large button.main:hover:not(:disabled),
  .large label.main:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
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
