<script lang="ts">
  // The navigation controls (Display: Navigation controls; the owner, 2026-11-01, beside ContentControls): back, and
  // beside it the page's name where the page is an index, the Library's and Discover's picker, or the panel's left part
  // where it handed its glass (lib/glass), its breadcrumbs or name. At the top or along the bottom. Two parts
  // (the owner, 2026-11-02): the buttons and one glass behind them, a title beside back that glass stretched (it was a
  // thinner glass of its own, the button's in front: the button changed colour as it went). A glass part (lib/glassMotion): as
  // what it holds changes, its glass reaches from the old outline to the new, the pill beside it doing the same as the
  // two share the row.
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import RecordsPicker from './RecordsPicker.svelte';
  import DiscoverPicker from './DiscoverPicker.svelte';
  import { glassPart } from './glassMotion';
  import { rowPiece } from './glassRow.svelte';

  let {
    left = null,
    back = null,
    name = null,
    picker = null,
    el = $bindable(),
  }: {
    /** The panel's left part, where it handed its glass: in place of the rest. */
    left?: Snippet | null;
    /** Back's tap; none, no back. */
    back?: (() => void) | null;
    /** The page's name, where the bar's tabs do not show it (a phone's index page). */
    name?: string | null;
    /** What the page shows, as a control page's name (the owner, 2026-10-25): the Library's kind of record, Discover's
     *  mode (the owner, 2026-10-29). */
    picker?: 'library' | 'discover' | null;
    el?: HTMLElement;
  } = $props();

  // The title beside back (the page's name, the Library's or Discover's picker) is a piece of its row (lib/glassRow), as a
  // breadcrumb's current level: cut with … for want of room, gone where it would be cut to a few letters, and gone
  // whole while the search is open (the owner, 2026-11-03). One decision for the row: nothing here measures the pill.
</script>

<span class="lead" bind:this={el} {@attach glassPart({ radius: 26 })}>
  <span class="shade" aria-hidden="true"></span><span class="glass" aria-hidden="true"></span>
  {#if left}
    {@render left()}
  {:else}
    {#if back}<span class="navpill"><button aria-label="Back" title="Back" onclick={back}><Icon name="back" size={22} /></button></span>{/if}
    {#if name}<h1 class="page-name" {@attach rowPiece(() => ({ id: `title:${name}`, rank: 10, min: 48, yields: true }))}>{name}</h1>{/if}
    {#if picker}<span class="title-box" {@attach rowPiece(() => ({ id: `title:${picker}`, rank: 10, min: 48, yields: true }))}
        >{#if picker === 'library'}<RecordsPicker />{:else}<DiscoverPicker />{/if}</span
      >{/if}
  {/if}
</span>

<style>
  /* One height whatever it holds (the owner, 2026-10-26: the glass moved a few pixels between the Library, with its
     picker, and Home, without). */
  .lead {
    position: relative;
    isolation: isolate;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    /* As tall as the pill's glass (lib/ContentControls: its 44 px buttons and 2 px), back's 2 px its own. */
    min-height: 48px;
    padding: 0;
  }

  /* No back at its start (a picker, a name): their room from the glass's round end. */
  .lead:not(:has(:global(.navpill))) {
    padding-left: 12px;
  }

  /* Two parts (the owner, 2026-11-02): the buttons and the glass behind them. A title or a picker beside back is that
     glass stretched, never a glass of its own beside the button's: it grows out of the button and shrinks back into it
     (lib/glassMotion), its colour the button's throughout ("I don't want this button to change the color when the
     extension removes"). Hidden only where nothing is in it. */
  .lead > .glass,
  .lead > .shade {
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: 26px;
    opacity: 0;
    pointer-events: none;
  }

  /* The part's back layer is the thin glass, which an extension is (the owner, 2026-11-03: "thin glass, one part");
     the main buttons have the glass over it (below). */
  .lead > .glass {
    background: var(--float-thin-bg);
    -webkit-backdrop-filter: var(--float-thin-blur);
    backdrop-filter: var(--float-thin-blur);
  }

  .lead > .shade {
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  /* A panel's left part (`left`) is not this file's markup: matched as anyone's. */
  .lead:has(> :global(:not(.glass, .shade, [data-glass-ghost]))) > :is(.glass, .shade) {
    opacity: 1;
  }

  /* A title in it: the glass stretched past it. Not a fading copy's (lib/glassMotion), nor a title given way (its row's
     decision, lib/glassRow). */
  .lead:has(:global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost], [data-glass-ghost] *, [data-row-gone], [data-row-gone] *))) {
    padding-right: 18px;
  }

  /* The main buttons (Back, Home, ✕; a panel's left part brings them as a `navpill`): the main glass, over the back
     layer; whatever beside them is the extension, on the thin glass alone. Never changing as the extension comes and
     goes ("I don't want this button to change the color when the extension removes"). */
  .lead :global(.navpill) {
    flex: none;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: none;
  }

  .lead :global(.navpill button) {
    flex: none;
  }

  /* The picker as a title: cut as a name is. */
  .title-box {
    display: flex;
    flex: 0 1 auto;
    min-width: 0;
  }

  .page-name {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 22px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Back, round as the pill (lib/ContentControls), on the lead's glass; never squeezed by a narrow row. */
  .navpill {
    display: flex;
    flex: none;
    padding: 2px;
    border-radius: 26px;
  }

  .navpill button {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }
</style>
