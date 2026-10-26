<script lang="ts">
  // A Library list as Flow (the owner, 2026-10-23: digging through the albums; its artists and tracks too, 2026-10-26):
  // the coverflow as tall as the page allows, the front one named under it with Play and Open. A tap on the front cover
  // opens it (`onopen`), one beside it comes to the front. One's own picture stands behind the whole page (the owner,
  // 2026-10-26: "should cover the entire page always"), fixed under the top bar and as wide as the page, the covers'
  // side fades reaching its edges past the page's gutter, the name light on it in either theme.
  import CoverFlow from './CoverFlow.svelte';
  import Icon from './Icon.svelte';
  import { backdropUrl } from './backdrop.svelte';

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
  }: {
    covers: string[];
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
  } = $props();

  /** The stage fills the page down to the mini-player's place, leaving room for the name and buttons under it (the
   *  owner, 2026-10-23: the space used). */
  const CAPTION = 116;
  /** Room kept for the mini-player at the window's foot, there or not (the owner, 2026-10-23: assume it). */
  const MINI = 96;
  let stageHeight = $state(420);
  /** What the coverflow leaves unused over its covers: the stage and the name move up by half, so they sit together in
   *  the middle of the room. */
  let spare = $state(0);
  const wall = $derived(!!backdropUrl());

  function fill(node: HTMLElement) {
    const measure = () => {
      const bottom = window.innerHeight - MINI;
      stageHeight = Math.max(260, Math.round(bottom - node.getBoundingClientRect().top - CAPTION));
    };
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

{#if wall}<div class="wall" {@attach besidePage} aria-hidden="true"></div>{/if}
<div class="flow" class:on-wall={wall} style:transform="translateY({-spare / 2}px)">
  <div class="stage" {@attach fill} style:height="{stageHeight}px">
    <CoverFlow {covers} top={12} foot={12} {onopen} {marked} {onfront} onspare={(px) => (spare = px)} {onmenu} />
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
    background:
      linear-gradient(to bottom, rgb(0 0 0 / 0.35), rgb(0 0 0 / 0.6)),
      var(--flow-photo) center 20% / cover no-repeat,
      var(--surface);
    pointer-events: none;
  }

  .stage {
    position: relative;
  }

  .flow.on-wall .stage {
    margin-inline: calc(-1 * var(--gutter, 0px));
  }

  /* The front one: its name large, who or what under it, then Play and Open as Home's buttons. */
  .caption {
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

  .flow.on-wall .named .title {
    color: #fff;
  }

  .flow.on-wall .named .by {
    color: rgb(255 255 255 / 0.75);
  }
</style>
