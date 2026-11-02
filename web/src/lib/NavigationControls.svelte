<script lang="ts">
  // The navigation controls (Display: Navigation controls; the owner, 2026-11-01, beside ContentControls): back, and
  // beside it the page's name where the page is an index, the Library's and Discover's picker, or the panel's left part
  // where it handed its glass (lib/glass), its breadcrumbs or name. At the top or along the bottom. Two parts
  // (the owner, 2026-11-02): the buttons and one glass behind them, a title beside back that glass stretched (it was a
  // thinner glass of its own, the button's in front: the button changed colour as it went). A glass part (lib/glassMotion): as
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
  /** The width a glass's children take laid out side by side (its padding and gaps too), without their motion: not its
   *  layers, nor the copies of what leaves (lib/glassMotion). */
  function heldWidth(box: HTMLElement): number {
    const s = getComputedStyle(box);
    const kids = [...box.children].filter((e): e is HTMLElement => e instanceof HTMLElement && !e.matches('.glass, .shade, [data-glass-ghost]'));
    const gap = parseFloat(s.columnGap) || 0;
    return kids.reduce((n, e) => n + e.offsetWidth, 0) + Math.max(0, kids.length - 1) * gap + (parseFloat(s.paddingLeft) || 0) + (parseFloat(s.paddingRight) || 0);
  }
  // Said to the pill beside it: the open search then takes the room the title left, to the gap beside back.
  $effect(() => {
    pageView.leadAway = away;
    return () => (pageView.leadAway = false);
  });
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
      // The open field squeezed narrower than it wants (lib/ContentControls' `data-wants`): the owner, 2026-11-01,
      // "Why did records not collapse??" beside a field cut short.
      const field = r?.querySelector<HTMLElement>('.field');
      const squeezed = !!field && field.getBoundingClientRect().width < Number(field.dataset.wants ?? 0) - 2;
      // Or the pill narrower than what it holds (its buttons never squeezed, its work's line never cut: the owner,
      // 2026-11-02). By what it holds laid out, not its scrolled width: what slides in (the field from the lens) counts
      // there at its start, and the title went away beside room to spare (the owner, 2026-11-02, Discover on the tablet).
      const pill = r?.querySelector<HTMLElement>('[role="toolbar"]');
      const work = r?.querySelector<HTMLElement>('.work');
      const over = (!!pill && heldWidth(pill) > pill.clientWidth + 1) || (!!work && work.scrollWidth > work.clientWidth + 1);
      away = parts.length > 0 && (met || squeezed || over || cut > 36 || room < 48 || room < 0.7 * (room + cut));
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

<span class="lead" class:title-away={away} bind:this={el} {@attach glassPart({ radius: 26 })}>
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

  .lead > .glass {
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
  }

  .lead > .shade {
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  /* A panel's left part (`left`) is not this file's markup: matched as anyone's. */
  .lead:has(> :global(:not(.glass, .shade, [data-glass-ghost]))) > :is(.glass, .shade) {
    opacity: 1;
  }

  /* A title in it: the glass stretched past it. Not a fading copy's (lib/glassMotion), nor a title given way. */
  .lead:not(.title-away):has(:global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost], [data-glass-ghost] *))) {
    padding-right: 18px;
  }

  /* The buttons on the lead's glass: none of their own (a panel's left part brings its back as a `navpill`). */
  .lead :global(.navpill) {
    flex: none;
    background: none;
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
    box-shadow: none;
  }

  .lead :global(.navpill button) {
    flex: none;
  }

  /* The search open with no room for the title (`away`; not the class `away`, app.css's for what slides away): only the
     round button. The panel's left part hides its own (its breadcrumbs, its name); the page's are not drawn. Not inside
     a leaving copy (lib/glassMotion): its picker hidden, its words fell to the top of its box, above back's middle
     (the owner, 2026-11-02). */
  .lead.title-away :global(:is(.page-name, .crumbs, .title, .views):not([data-glass-ghost] *)) {
    display: none;
  }

  .page-name {
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
