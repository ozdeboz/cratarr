<script lang="ts" module>
  import type { IconName } from './Icon.svelte';
  /** `busy`: under way, the app's busy ring in its place (Change cover's look, at the top right). */
  /** `on`: a state it shows (Repeat in the queue's sheet): pressed, in the accent. */
  export type SheetAction = { icon: IconName; label: string; onclick: () => void; disabled?: boolean; busy?: boolean; on?: boolean };
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
  import Cover, { coverBroke, coverBroken, mosaicOf } from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { accentOf } from './remote.svelte';
  import { sheetArea } from './SheetArea.svelte';
  import { sheetDrag } from './sheetDrag';
  import { toast } from './toast.svelte';
  import { copyText } from './clipboard';

  let {
    label,
    onclose,
    images = null,
    icon = null,
    title,
    chips = [],
    heart = null,
    end = null,
    actions = [],
    onback = null,
    beside = null,
    step = 'menu',
    over = false,
    copy = null,
    children,
  }: {
    label: string;
    onclose: () => void;
    /** The item's image: one cover, or up to four for a playlist's mosaic (2 × 2); [] while it loads. Without (null,
     *  the default) the sheet has no image at all (the legend, a name to enter): the card is only its title and chips. */
    images?: string[] | null;
    /** Before the title on a card without an image, as Symfonium heads a sheet that is not about one item (the
     *  source filter's funnel). */
    icon?: IconName | null;
    title: string;
    chips?: string[];
    heart?: { on: boolean; disabled?: boolean; toggle: () => void } | null;
    /** An action at the title's end where there is no heart (the Filter sheet's Save as crate), or a few side by side
     *  (Change cover's From an address and Upload). */
    end?: SheetAction | SheetAction[] | null;
    actions?: SheetAction[];
    /** In a step: back to the entries (Esc too). */
    onback?: (() => void) | null;
    /** In the back button's row, beside it: how far a step's work is (Change cover's look). */
    beside?: Snippet | null;
    /** Which step shows: a change fades the new content in (500 ms) while the sheet resizes. */
    step?: string;
    /** Over the full-screen visuals (lib/visuals), which lie over everything else. */
    over?: boolean;
    /** What a tap on the item (its cover, title and chips) copies: the usual "Artist – Title" (the owner, 2026-10-24,
     *  Discover's sheet). Without it the item is not a button. */
    copy?: string | null;
    children: Snippet;
  } = $props();

  /** The item copied (its `copy`), said as the share's copy says it. */
  async function copyItem() {
    if (!copy) return;
    if (await copyText(copy)) toast.show({ text: 'Copied', tone: 'done' });
    else toast.show({ text: "It couldn't be copied", tone: 'error' });
  }

  // The accent from the first image, fading in once known.
  let accent = $state<string | null>(null);
  $effect(() => {
    const src = images?.[0];
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

  /** A press began on the backdrop (since the sheet opened). */
  let pressedHere = false;

  // Dragged down from anywhere on it, it closes (up, for one at the top); a short drag springs back (lib/sheetDrag).
  let dy = $state(0);

  // The content grows and shrinks smoothly between the entries and a step (Finamp's inline panels).
  let inner = $state<HTMLElement>();
  let height = $state<number | null>(null);
  $effect(() => {
    if (!inner) return;
    const ro = new ResizeObserver(() => (height = inner!.offsetHeight));
    ro.observe(inner);
    return () => ro.disconnect();
  });

  // The side it belongs to (the page's or the panel's, lib/SheetArea.svelte): with both in view, the sheet stands over
  // that side and dims only it; a sheet of neither stays in the middle of the window.
  const area = sheetArea();
  let side = $state<{ left: number; width: number } | null>(null);
  $effect(() => {
    if (!area) return;
    const measure = () => {
      const r = area.getBoundingClientRect();
      side = r.width > 0 ? { left: r.left, width: r.width } : null;
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(area);
    addEventListener('resize', measure);
    return () => {
      ro.disconnect();
      removeEventListener('resize', measure);
    };
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
<!-- Closes on a press that began on it: not on the lift of the long-press that opened the sheet (lib/longpress), which
     lands here once the sheet is up. -->
<div
  class="backdrop"
  class:sided={side !== null}
  style:z-index={over ? 2010 : null}
  role="presentation"
  onpointerdown={() => (pressedHere = true)}
  onclick={() => pressedHere && onclose()}
  transition:fade|global={{ duration: SHEET_OUT }}
>
  {#if side}<div class="dim" style:left="{side.left}px" style:width="{side.width}px"></div>{/if}
</div>
<div
  class="sheet"
  style:z-index={over ? 2011 : null}
  role="dialog"
  aria-modal="true"
  aria-label={label}
  tabindex="-1"
  style:--art-accent={accent ?? 'var(--accent)'}
  style:--side-w={side ? `${side.width}px` : null}
  style:left={side ? `${side.left + side.width / 2}px` : null}
  style:transform={dy ? `translateY(${dy}px)` : null}
  class:dragging={dy !== 0}
  bind:this={sheet}
  use:sheetDrag={{ onclose, ondrag: (d) => (dy = d) }}
  in:fly|global={{ y: 400, duration: SHEET_IN, easing: decelerate }}
  out:fly|global={{ y: 400, duration: SHEET_OUT, easing: decelerate }}
>
  {#if images?.[0] && !coverBroken(images[0])}<img class="blur" src={images[0]} alt="" aria-hidden="true" onerror={() => coverBroke(images?.[0])} />{/if}
  <div class="veil"></div>
  <div class="handle" aria-hidden="true"></div>

  <div class="card" class:bare={images === null}>
    <svelte:element
      this={copy ? 'button' : 'div'}
      class="item"
      class:copies={!!copy}
      role={copy ? undefined : 'presentation'}
      aria-label={copy ? `Copy “${copy}”` : undefined}
      title={copy ? 'Copy' : undefined}
      onclick={copy ? copyItem : undefined}
    >
    {#if images === null}
      <!-- no image: the title and chips alone, the icon before them when there is one -->
      {#if icon}<span class="lead"><Icon name={icon} size={24} /></span>{/if}
    {:else if mosaicOf(images).length >= 4}
      <div class="art mosaic">{#each mosaicOf(images) as src, i (i)}<Cover {src} />{/each}</div>
    {:else}
      <!-- no cover, or one that fails: the stand-in (lib/Cover) -->
      <span class="art"><Cover src={mosaicOf(images)[0]} /></span>
    {/if}
    <div class="info">
      <div class="title">{title}</div>
      {#if chips.length}<div class="chips">{chips.join(' • ')}</div>{/if}
    </div>
    </svelte:element>
    {#if heart}
      <button class="heart" class:on={heart.on} disabled={heart.disabled} aria-label={heart.on ? 'Favourite' : 'Not a favourite'} aria-pressed={heart.on} onclick={heart.toggle}
        ><Icon name="heart" size={24} /></button
      >
    {:else if end}
      {#each Array.isArray(end) ? end : [end] as x (x.label)}
        <button class="heart" class:lit={x.on} aria-label={x.label} title={x.label} disabled={x.disabled} aria-busy={x.busy || undefined} aria-pressed={x.on ?? undefined} onclick={x.onclick}><Icon name={x.icon} size={24} /></button>
      {/each}
    {/if}
  </div>

  <div class="body" style:height={height === null ? null : `${height}px`}>
    <div class="content" bind:this={inner}>
      {#if onback}
        <div class="back-row">
          <button class="back" aria-label="Back" onclick={onback}><Icon name="back" size={22} /></button>
          {#if beside}<span class="beside">{@render beside()}</span>{/if}
        </div>
      {:else if actions.length}
        <div class="actions">
          {#each actions as a (a.label)}
            <button class:play={a.icon === 'play'} aria-label={a.label} title={a.label} disabled={a.disabled} onclick={a.onclick}><Icon name={a.icon} size={28} /></button>
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

  /* A sheet of one side: only that side dims; a tap anywhere else still closes it. */
  .backdrop.sided {
    background: none;
  }

  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgb(0 0 0 / 0.54);
  }

  /* Finamp's sheet: at most 500 px wide, centred (in its side, or the window), top corners 20 px. With the keyboard up,
     just above it (lib/keyboard's --kb, as the bottom glass; the owner, 2026-10-25: every sheet to type in this way,
     none from the top). */
  .sheet {
    position: fixed;
    left: 50%;
    bottom: var(--kb, 0px);
    z-index: 41;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    width: min(500px, var(--side-w, 100vw));
    max-height: calc(92dvh - var(--kb, 0px));
    margin-left: calc(min(500px, var(--side-w, 100vw)) / -2);
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

  /* The item at the top, as Symfonium's sheet heads (decided 2026-10-16): a small cover, the title large beside it and
     what it is under that in one muted line, on the sheet itself. */
  .card {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    margin: 4px 16px 8px;
  }

  /* No image: the title and its line alone. */
  /* The item (cover, title, chips): a button where a tap copies it. */
  .item {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 16px;
    min-width: 0;
  }

  button.item {
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: copy;
  }

  .card.bare .info {
    gap: 6px;
    padding: 2px 0 6px;
  }

  .lead {
    display: grid;
    flex: none;
    place-items: center;
  }

  .art {
    flex: none;
    display: block;
    width: 64px;
    height: 64px;
    overflow: hidden;
    border-radius: 6px;
  }

  .mosaic {
    display: grid;
    grid-template: 1fr 1fr / 1fr 1fr;
    overflow: hidden;
  }

  .info {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    font-size: 21px;
    line-height: 1.15;
    overflow-wrap: anywhere;
  }

  .chips {
    overflow: hidden;
    color: var(--muted);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .heart {
    flex: none;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-right: -10px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }

  .heart:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .heart.lit {
    color: var(--accent-ink);
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

  /* Symfonium's playback row: the actions spread evenly, icons only (their words on a long-press, as every icon
     button's), Play in a ring of the cover's accent. */
  .actions {
    display: flex;
    justify-content: space-evenly;
    align-items: center;
    height: 76px;
    padding: 0 4px;
  }

  .actions button {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 50%;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .actions button.play {
    box-shadow: inset 0 0 0 2px var(--art-accent);
    color: var(--art-accent);
  }

  .actions button:hover,
  .actions button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .back-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-right: 16px;
  }

  .beside {
    flex: 1;
    min-width: 0;
    margin-bottom: 4px;
  }

  .back {
    flex: none;
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
