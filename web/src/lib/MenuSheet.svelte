<script lang="ts" module>
  import type { IconName } from './Icon.svelte';
  export type SheetAction = { icon: IconName; label: string; onclick: () => void; disabled?: boolean };
</script>

<script lang="ts">
  import { portal } from './portal';
  // Every menu (decision 0021), as Finamp's track menu: a bottom sheet with the item on a card at its top (its image,
  // title, chips; for a track the heart), a row of large actions, then the entries (lib/MenuEntry.svelte) or a step
  // (a form, a list) given as children. It rises from the bottom; a tap on the backdrop, Esc or dragging it down closes
  // it. It takes its look from the item's image: blurred behind it, and its accent.
  import type { Snippet } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { decelerate, SHEET_IN, SHEET_OUT } from './motion';
  import Icon from './Icon.svelte';
  import { accentOf } from './remote.svelte';

  let {
    label,
    onclose,
    images = [],
    title,
    chips = [],
    heart = null,
    actions = [],
    onback = null,
    step = 'menu',
    children,
  }: {
    label: string;
    onclose: () => void;
    /** The item's image: one cover, or up to four for a playlist's mosaic (2 × 2). */
    images?: string[];
    title: string;
    chips?: string[];
    heart?: { on: boolean; disabled?: boolean; toggle: () => void } | null;
    actions?: SheetAction[];
    /** In a step: back to the entries (Esc too). */
    onback?: (() => void) | null;
    /** Which step shows: a change fades the new content in (500 ms) while the sheet resizes. */
    step?: string;
    children: Snippet;
  } = $props();

  // The accent from the first image, fading in once known.
  let accent = $state<string | null>(null);
  $effect(() => {
    const src = images[0];
    accent = null;
    if (!src) return;
    let gone = false;
    accentOf(src).then((a) => !gone && (accent = a));
    return () => (gone = true);
  });

  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      if (onback) onback();
      else onclose();
    };
    addEventListener('keydown', key, true);
    return () => removeEventListener('keydown', key, true);
  });

  // Dragging the sheet down past 80 px closes it; less springs back.
  let dy = $state(0);
  let from: { y: number; id: number } | null = null;
  function down(e: PointerEvent) {
    if ((e.target as Element).closest('button, input, select, textarea, a, .scroll')) return;
    from = { y: e.clientY, id: e.pointerId };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (from && e.pointerId === from.id) dy = Math.max(0, e.clientY - from.y);
  }
  function up() {
    if (!from) return;
    from = null;
    if (dy > 80) onclose();
    dy = 0;
  }

  // The content grows and shrinks smoothly between the entries and a step (Finamp's inline panels).
  let inner = $state<HTMLElement>();
  let height = $state<number | null>(null);
  $effect(() => {
    if (!inner) return;
    const ro = new ResizeObserver(() => (height = inner!.offsetHeight));
    ro.observe(inner);
    return () => ro.disconnect();
  });

  // Focus the first control on open, so the keyboard lands in the menu.
  let sheet = $state<HTMLElement>();
  $effect(() => {
    sheet?.querySelector<HTMLElement>('.content button, .content input')?.focus({ preventScroll: true });
  });
</script>

<!-- At the end of the page (lib/portal.ts): over every layer it was opened from. Global transitions: a sheet slides in and out however it is opened (inside another block, such as TrackMenu's
     {#if}, a local one would not play). -->
<div class="root" use:portal>
<div class="backdrop" role="presentation" onclick={onclose} transition:fade|global={{ duration: SHEET_OUT }}></div>
<div
  class="sheet"
  role="dialog"
  aria-modal="true"
  aria-label={label}
  tabindex="-1"
  style:--art-accent={accent ?? 'var(--accent)'}
  style:transform={dy ? `translateY(${dy}px)` : null}
  class:dragging={dy > 0}
  bind:this={sheet}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  in:fly|global={{ y: 400, duration: SHEET_IN, easing: decelerate }}
  out:fly|global={{ y: 400, duration: SHEET_OUT, easing: decelerate }}
>
  {#if images[0]}<img class="blur" src={images[0]} alt="" aria-hidden="true" />{/if}
  <div class="veil"></div>
  <div class="handle" aria-hidden="true"></div>

  <div class="card">
    {#if images.length >= 4}
      <div class="art mosaic">{#each images.slice(0, 4) as src, i (i)}<img {src} alt="" />{/each}</div>
    {:else if images[0]}
      <img class="art" src={images[0]} alt="" />
    {/if}
    <div class="info">
      <div class="title">{title}</div>
      {#if chips.length}
        <div class="chips scroll">{#each chips as c, i (i)}<span>{c}</span>{/each}</div>
      {/if}
    </div>
    {#if heart}
      <button class="heart" class:on={heart.on} disabled={heart.disabled} aria-label={heart.on ? 'Favourite' : 'Not a favourite'} aria-pressed={heart.on} onclick={heart.toggle}
        ><Icon name="heart" size={20} /></button
      >
    {/if}
  </div>

  <div class="body" style:height={height === null ? null : `${height}px`}>
    <div class="content" bind:this={inner}>
      {#if onback}
        <button class="back" aria-label="Back" onclick={onback}><Icon name="back" size={22} /></button>
      {:else if actions.length}
        <div class="actions">
          {#each actions as a (a.label)}
            <button disabled={a.disabled} onclick={a.onclick}><Icon name={a.icon} size={28} /><span>{a.label}</span></button>
          {/each}
        </div>
      {/if}
      {#key step}
        <div in:fade={{ duration: 500 }}>{@render children()}</div>
      {/key}
    </div>
  </div>
</div>
</div>

<style>
  /* Only a carrier: the backdrop and the sheet place themselves. */
  .root {
    display: contents;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgb(0 0 0 / 0.54);
  }

  /* Finamp's sheet: at most 500 px wide, centred, top corners 20 px. */
  .sheet {
    position: fixed;
    left: 50%;
    bottom: 0;
    z-index: 41;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    width: min(500px, 100vw);
    max-height: 92dvh;
    margin-left: calc(min(500px, 100vw) / -2);
    padding-bottom: calc(16px + env(safe-area-inset-bottom));
    overflow: hidden;
    border-radius: 20px 20px 0 0;
    background: var(--surface);
    color: var(--text);
    touch-action: none;
    transition: transform 200ms linear;
  }

  .sheet.dragging {
    transition: none;
  }

  .blur {
    position: absolute;
    inset: -20%;
    width: 140%;
    height: 140%;
    object-fit: cover;
    filter: blur(60px) saturate(1.2);
    pointer-events: none;
  }

  .veil {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--surface) 70%, transparent);
    pointer-events: none;
  }

  .handle {
    position: relative;
    flex: none;
    width: 40px;
    height: 3.5px;
    margin: 16px auto 10px;
    border-radius: 3.5px;
    background: var(--text);
    opacity: 0.6;
  }

  /* The item on a card: image 140 × 140 on the left, the card rounding its corners. */
  .card {
    position: relative;
    flex: none;
    display: flex;
    height: 140px;
    margin: 0 12px 10px;
    overflow: hidden;
    border-radius: 12px;
    background: rgb(0 0 0 / 0.25);
  }

  :global([data-theme='light']) .card {
    background: rgb(255 255 255 / 0.15);
  }

  .art {
    flex: none;
    width: 140px;
    height: 140px;
    object-fit: cover;
  }

  .mosaic {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .mosaic img {
    width: 70px;
    height: 70px;
    object-fit: cover;
  }

  .info {
    display: flex;
    flex: 1;
    flex-direction: column;
    justify-content: space-evenly;
    min-width: 0;
    padding: 0 26px 0 12px;
  }

  .title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    font-size: 18px;
    line-height: 1.1;
    overflow-wrap: anywhere;
  }

  .chips {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .chips span {
    flex: none;
    padding: 4px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    font-size: 0.8rem;
    white-space: nowrap;
  }

  .heart {
    position: absolute;
    right: 4px;
    bottom: 4px;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }

  .heart.on :global(svg) {
    fill: currentColor;
  }

  .body {
    position: relative;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    transition: height 500ms ease-in-out;
  }

  /* Finamp's playback row: 96 px, the actions spread evenly, a 28 px icon over a small light label, no fill. */
  .actions {
    display: flex;
    justify-content: space-evenly;
    height: 96px;
    padding: 0 4px;
  }

  .actions button {
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    align-items: center;
    gap: 9px;
    padding: 10px 8px 12px;
    background: none;
    border: 0;
    border-radius: 12px;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .actions button span {
    font-size: 12px;
    font-weight: 300;
    line-height: 1.4;
    text-align: center;
  }

  .actions button:hover,
  .actions button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .back {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin: 0 0 4px 12px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }
</style>
