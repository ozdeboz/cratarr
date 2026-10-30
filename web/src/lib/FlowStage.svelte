<script lang="ts" module>
  /** The last flow's measures: the next one (Albums to Artists, the owner 2026-10-26: it shifted) starts from them, not
   *  from a guess. */
  const last = { stage: 420, spare: 0 };
</script>

<script lang="ts">
  // A Library list as Flow (the owner, 2026-10-23: digging through the albums; its artists and tracks too, 2026-10-26):
  // the coverflow as tall as the page allows, the front one named under it with Play and Open. A tap on the front cover
  // opens it (`onopen`), one beside it comes to the front. One's own picture stands behind the whole page (the owner,
  // 2026-10-26: "should cover the entire page always"), fixed under the top bar and as wide as the page, the covers'
  // side fades reaching its edges past the page's gutter, the name light on it in either theme.
  import { untrack } from 'svelte';
  import CoverFlow from './CoverFlow.svelte';
  import Icon from './Icon.svelte';
  import CoverFacts, { type CoverFact } from './CoverFacts.svelte';
  import { backdropUrl, flowOpened } from './backdrop.svelte';

  let {
    covers,
    marked,
    front,
    onopen,
    onfront,
    onmenu,
    onname,
    onplay,
    playing = false,
    canPlay = true,
    details = null,
    seed,
  }: {
    covers: string[];
    /** What names this flow, for Shuffle's photo of its own (lib/backdrop). */
    seed: string;
    /** The ticked ones, by their place (Select). */
    marked?: boolean[];
    /** The front one's name and who or what under it. */
    front: { title: string; by: string } | null;
    onopen: (i: number) => void;
    onfront: (i: number) => void;
    onmenu: (i: number, rect: DOMRect) => void;
    /** Its name and Open. */
    onname: () => void;
    onplay: () => void;
    /** Starting to play. */
    playing?: boolean;
    canPlay?: boolean;
    /** The front one's facts, shown over its cover by Details (the owner, 2026-10-26: as the player's info). */
    details?: { genres: string[]; lines: CoverFact[] } | null;
  } = $props();

  /** The stage fills the page down to the mini-player's place, leaving room for the name and buttons under it (the
   *  owner, 2026-10-23: the space used). */
  const CAPTION = 116;
  /** Room kept for the mini-player at the window's foot, there or not (the owner, 2026-10-23: assume it). */
  const MINI = 96;
  let stageHeight = $state(last.stage);
  /** What the coverflow leaves unused over its covers: the stage and the name move up by half (taking that room with
   *  them, no scroll left under them), so they sit together in the middle of the room. */
  let spare = $state(last.spare);
  const wall = $derived(!!backdropUrl());
  // Shuffle's next on opening (Settings › Appearance).
  $effect(() => {
    const s = seed;
    untrack(() => flowOpened(s));
  });
  /** Details on: the front one's facts over its cover, until another comes to the front. */
  let showing = $state(false);
  function toFront(i: number) {
    showing = false;
    onfront(i);
  }

  const px = (el: Element | null, side: string) => (el ? parseFloat(getComputedStyle(el).getPropertyValue(side)) || 0 : 0);
  // Down to what lies under it (the owner, 2026-10-26: no room to scroll with nothing in it): its own padding, the
  // list's, and the page's (the bars, the mini-player's room: at least MINI). Measured from where it stands before it
  // moves up by half its spare room, so measuring again finds the same (it grew and jumped 300 ms in).
  function fill(node: HTMLElement) {
    const measure = () =>
      untrack(() => {
        const top = node.getBoundingClientRect().top + spare / 2;
        const flow = node.parentElement;
        const caption = flow?.querySelector('.caption')?.getBoundingClientRect().height || CAPTION;
        const under = px(flow, 'padding-bottom') + px(node.closest('main'), 'padding-bottom') + Math.max(MINI, px(node.closest('.page'), 'padding-bottom'));
        stageHeight = last.stage = Math.max(260, Math.floor(window.innerHeight - top - caption - under));
      });
    measure();
    const later = setTimeout(measure, 300);
    addEventListener('resize', measure);
    return () => {
      clearTimeout(later);
      removeEventListener('resize', measure);
    };
  }
  /** The picture as wide as the page (beside a sidebar or a player, the page's own part). */
  function besidePage(el: HTMLElement) {
    const page = el.parentElement?.closest('.page') as HTMLElement | null;
    if (!page) return;
    const place = () => {
      const r = page.getBoundingClientRect();
      el.style.left = `${r.left}px`;
      el.style.width = `${r.width}px`;
    };
    place();
    const watch = new ResizeObserver(place);
    watch.observe(page);
    addEventListener('resize', place);
    return () => {
      watch.disconnect();
      removeEventListener('resize', place);
    };
  }
</script>

{#snippet facts()}{#if details}<CoverFacts genres={details.genres} lines={details.lines} />{/if}{/snippet}

{#if wall}<div class="wall flow-wall" {@attach besidePage} aria-hidden="true"></div>{/if}
<div class="flow" class:on-wall={wall} style:margin-top="{-spare / 2}px">
  <div class="stage" {@attach fill} style:height="{stageHeight}px">
    <CoverFlow {covers} top={12} foot={12} bleed={48} {onopen} {marked} onfront={toFront} front={showing && details ? facts : null} onspare={(px) => (spare = last.spare = px)} {onmenu} />
  </div>
  {#if front}
    <div class="caption">
      <button class="named" onclick={onname}>
        <span class="title">{front.title}</span>
        <span class="by">{front.by}</span>
      </button>
      <div class="acts">
        <button class="main" disabled={!canPlay} aria-busy={playing} onclick={onplay}><Icon name="play" size={18} />Play</button>
        <button onclick={onname}>Open</button>
        {#if details}<button class:on={showing} aria-pressed={showing} onclick={() => (showing = !showing)}><Icon name="info" size={18} />Details</button>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .flow {
    padding: 4px 0 12px;
  }

  .wall {
    position: fixed;
    top: calc(var(--top-h, 0px) + env(safe-area-inset-top));
    bottom: 0;
    z-index: -1;
    background: var(--bg);
    pointer-events: none;
  }

  /* The photo under its veil once it is in (App); till then the page's own ground. */
  :global(:root[data-flow-photo]) .wall {
    background:
      linear-gradient(to bottom, rgb(0 0 0 / 0.35), rgb(0 0 0 / 0.6)),
      var(--flow-photo) center 20% / cover no-repeat,
      var(--bg);
  }

  /* The body's own background painted over it (behind the page, it is under the body): while it shows, the body lets
     it through; the page's ground is the root's, under the picture (the owner, 2026-10-26: a black box over it). */
  :global(body:has(.flow-wall)) {
    background: transparent;
  }

  .stage {
    position: relative;
  }

  .flow.on-wall .stage {
    margin-inline: calc(-1 * var(--gutter, 0px));
  }

  /* The front one: its name large, who or what under it, then Play and Open as Home's buttons. */
  /* Over the covers' bleed under the stage: its name and buttons take the taps there. */
  .caption {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 4px 16px 0;
  }

  .acts {
    display: flex;
    gap: 8px;
  }

  .acts button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .acts button.on {
    background: color-mix(in srgb, var(--text) 22%, transparent);
  }

  .acts button.main {
    background: var(--accent);
    color: var(--accent-text);
  }

  .acts button:disabled,
  .acts button[aria-busy='true'] {
    opacity: 0.6;
  }

  .named {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: center;
    cursor: pointer;
  }

  .named .title {
    max-width: 80vw;
    overflow: hidden;
    font-size: 1.35rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .named .by {
    color: var(--muted);
    font-size: 0.9rem;
  }

  :global(:root[data-flow-photo]) .flow.on-wall .named .title {
    color: #fff;
  }

  :global(:root[data-flow-photo]) .flow.on-wall .named .by {
    color: rgb(255 255 255 / 0.75);
  }
</style>
