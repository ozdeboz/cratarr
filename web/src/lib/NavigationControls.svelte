<script lang="ts">
  // The navigation controls (Display: Navigation controls; the owner, 2026-11-01, beside ContentControls): back, and
  // beside it the page's name where the page is an index, the Library's and Discover's picker, or the panel's left part
  // where it handed its glass (lib/glass), its breadcrumbs or name. At the top or along the bottom. A title has a
  // thinner glass than the button's, more light through (the owner, 2026-10-25, mockup 3: content scrolling under a
  // bare title made it hard to read), the button's round glass in front of it. A glass part (lib/glassMotion): as
  // what it holds changes, its glass reaches from the old outline to the new, the pill beside it doing the same as the
  // two share the row.
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import RecordsPicker from './RecordsPicker.svelte';
  import DiscoverPicker from './DiscoverPicker.svelte';
  import { glassPart } from './glassMotion';
  import { pageView } from './pageView.svelte';

  let {
    left = null,
    back = null,
    name = null,
    picker = null,
    still = false,
    right,
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
    /** No motion now (an index page's panel coming or going under the bar: the owner, 2026-10-25, no fade there). */
    still?: boolean;
    /** The pill's side of the row, to give way to. */
    right: () => HTMLElement | undefined;
    el?: HTMLElement;
  } = $props();

  // The title beside back gives way to the open search where it would be cut to a few letters or less (the owner,
  // 2026-10-25), and as the window or the pill's side changes (its globe coming): the pill is laid out as it ends at
  // once (lib/glassMotion), so it is measured then, before its glass moves. A title that fits beside the open field
  // stays as it is. Library's view (its picker is never cut: the field's glass slides over it) gives way the same
  // where the two glasses would meet (the owner, 2026-10-26).
  let away = $state(false);
  $effect(() => {
    if (!pageView.searchOpen) return void (away = false);
    const measure = () => {
      if (!el || away) return;
      const parts = [...el.querySelectorAll<HTMLElement>('.page-name, .crumbs .up, .crumbs .here, .title, .views')].filter((e) => !e.closest('[data-glass-ghost]'));
      const cut = parts.reduce((n, e) => n + Math.max(0, e.scrollWidth - e.clientWidth), 0);
      const room = parts.reduce((n, e) => n + e.clientWidth, 0);
      // More than a few letters cut (about three, or a third of it: "Req…"), or no room at all.
      // Its content's edge, not its glass's: a narrowed glass lets the picker run out under the field.
      const edge = Math.max(el.getBoundingClientRect().right, ...parts.map((e) => e.getBoundingClientRect().left + e.scrollWidth));
      const r = right();
      const met = !!r && edge > r.getBoundingClientRect().left - 4;
      // The open field squeezed narrower than it wants (its CSS: min(320px, 100vw - 220px)): the owner, 2026-11-01,
      // "Why did records not collapse??" beside a field cut short.
      const field = r?.querySelector<HTMLElement>('.field');
      const squeezed = !!field && field.getBoundingClientRect().width < Math.min(320, innerWidth - 220) - 2;
      away = parts.length > 0 && (met || squeezed || cut > 36 || room < 48 || room < 0.7 * (room + cut));
    };
    tick().then(measure);
    const r = right();
    const seen = new ResizeObserver(measure);
    if (r) seen.observe(r);
    addEventListener('resize', measure);
    return () => {
      seen.disconnect();
      removeEventListener('resize', measure);
    };
  });
</script>

<span class="lead" class:title-away={away} bind:this={el} {@attach glassPart({ radius: 26, still: () => still })}>
  <span class="shade" aria-hidden="true"></span><span class="glass" aria-hidden="true"></span>
  {#if left}
    {@render left()}
  {:else}
    {#if back}<span class="navpill"><button aria-label="Back" title="Back" onclick={back}><Icon name="back" size={22} /></button></span>{/if}
    {#if name && !away}<h1 class="page-name">{name}</h1>{/if}
    {#if picker === 'library' && !away}<RecordsPicker />{:else if picker === 'discover' && !away}<DiscoverPicker />{/if}
  {/if}
</span>

<style>
  /* Embedded always as with its extension (the owner, 2026-10-26: the glass moved a few pixels between the Library,
     with its view picker's thin glass, and Home, without): the extension's 2 px, whether it shows or not. */
  .lead {
    position: relative;
    isolation: isolate;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 2px;
  }

  .lead > .glass,
  .lead > .shade {
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: 26px;
    opacity: 0;
    pointer-events: none;
  }

  .lead > .glass {
    background: color-mix(in srgb, var(--float-bg) 45%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(1.3);
    backdrop-filter: blur(10px) saturate(1.3);
  }

  .lead > .shade {
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.18);
  }

  /* A title in it: its thin glass, the round button's glass in front of it. Not a fading copy's (lib/glassMotion). */
  .lead:has(:global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost], [data-glass-ghost] *))) {
    padding: 2px 18px 2px 2px;
  }

  .lead:has(:global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost], [data-glass-ghost] *))) > :is(.glass, .shade) {
    opacity: 1;
  }

  .lead:has(:global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost], [data-glass-ghost] *))) :global(.navpill) {
    box-shadow: none;
  }

  /* The search open with no room for the title (`away`; not the class `away`, app.css's for what slides away): only the round button. The panel's left part hides its own
     (its breadcrumbs, its name); the page's are not drawn. */
  .lead.title-away :global(:is(.page-name, .crumbs, .title, .views)) {
    display: none;
  }

  .lead.title-away {
    padding: 2px;
  }

  .lead.title-away > :is(.glass, .shade) {
    opacity: 0;
  }

  .lead.title-away :global(.navpill) {
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .page-name {
    margin: 0;
    overflow: hidden;
    font-size: 22px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Back, round as the pill (lib/ContentControls). */
  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
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
