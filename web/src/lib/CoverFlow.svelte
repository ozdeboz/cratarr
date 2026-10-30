<script lang="ts">
  // Albums to flip through, a coverflow (the owner, 2026-10-23: "more representative of digging through albums in a
  // visual way"): a crate's header, and the Library's Flow layout. Curved sideways, the front one facing, the ends
  // curving down a little; a drag sideways moves through all of them, a tap goes one on (with `onopen`: a tap on the
  // front one opens it, one beside it comes to the front), the arrow keys too, and they go round. It lays itself out
  // from the room it has: the front cover as large as the height allows between `top` and `foot`, the covers beside it
  // spread over the width (off its edges where it is narrow), the sides fading out. Only the covers in sight are made,
  // and the next few are fetched ahead.
  import { untrack, type Snippet } from 'svelte';
  import Cover from './Cover.svelte';

  let {
    covers,
    top: TOP = 64,
    foot: FOOT = 72,
    onopen,
    onfront,
    onspare,
    onmenu,
    marked,
    enter = false,
    bleed = 0,
    fade = false,
    front: frontLayer,
  }: {
    /** Come in when shown (Home's featured flow; the owner, 2026-10-26): from both edges towards the middle, the outer
     *  covers first, the front one last. */
    enter?: boolean;
    covers: string[];
    /** Which covers are highlighted, by their place (the ticked ones in select mode; the owner, 2026-10-23: a border). */
    marked?: boolean[];
    /** How far below its place it may still draw (the covers' lower edges and shadows, the owner 2026-10-26: no hard
     *  edge under them), the covers laid out as if it did not. */
    bleed?: number;
    /** Its sides fading out (the owner, 2026-10-23). Off everywhere since (the owner, 2026-10-26: every flow's picture
     *  reaches its edges, "no transparency gradient at the sides"); kept for a flow on a plain page. */
    fade?: boolean;
    /** Drawn over the front cover, which blurs under it (a flow's details, lib/FlowStage). */
    front?: Snippet | null;
    /** Room kept free along the top (a header's buttons) and over the foot (the fade under its action bar). */
    top?: number;
    foot?: number;
    /** A tap on the front cover: its place in `covers`. */
    onopen?: (i: number) => void;
    /** The front cover changed: its place. */
    onfront?: (i: number) => void;
    /** A long-press or a right-click: the front cover's menu, by its place (the owner, 2026-10-24: in every flow, as
     *  everywhere, decision 0058). */
    onmenu?: (i: number, rect: DOMRect) => void;
    /** The height the covers leave unused (they sit at the foot): what holds them can centre them with what follows. */
    onspare?: (px: number) => void;
  } = $props();

  // Its room, once measured: no cover placed before (the owner, 2026-10-26: switched to Flow, they came in jagged, laid
  // out for a guessed room and then sliding to the real one), none moving the frame they are first placed.
  let size = $state(0);
  let height = $state(0);
  const measured = $derived(size > 0 && height > 0);
  /** The covers whose picture is in: each shows (its frame and shadow too) only then (the owner, 2026-10-26: shadows
   *  before the covers), fading in. */
  const ready = $state<Record<string, true>>({});
  // Its room changing later (a page settling, a window resized) moves them at once too, not sliding.
  let settled = $state(false);
  $effect(() => {
    void size;
    void height;
    if (!measured) return;
    settled = false;
    const r = requestAnimationFrame(() => requestAnimationFrame(() => (settled = true)));
    return () => cancelAnimationFrame(r);
  });
  /** The front cover's place in `covers`, between whole places while dragged. */
  let pos = $state(0);
  let dragging = $state(false);
  const n = $derived(covers.length);
  /** One cover alone (a panel's header too wide to show all of it, the owner 2026-10-26): it faces, nothing beside it,
   *  nothing to move through (the covers would come round to it again on both sides). */
  const single = $derived(n === 1);
  const wrap = (i: number) => ((i % n) + n) % n;
  const clamp = (x: number, a: number, b: number) => Math.max(a, Math.min(b, x));

  type Place = { t: string; f: number; o: number; z: number; w: number };
  /** The layout from the room there is (the owner, 2026-10-23: responsive, the space used). */
  const front = $derived.by(() => {
    const room = height - bleed - TOP - FOOT - 8;
    // Its share of the width: most of a narrow place (the sides may go off its edges, the owner: "quite okay"), less of
    // a wide one, which then shows more covers beside it.
    const share = clamp(0.98 - size * 0.0005, 0.42, 0.8);
    const w = Math.max(80, Math.min(room, size * share));
    // Each side's covers spread from just beyond the front one's edge out to the place's edge (where they seem to be:
    // `place` undoes the depth), as many as fit.
    const half = size / 2;
    const first = w * 0.62;
    const sides = clamp(Math.ceil((half - w * 0.5) / (w * 0.26)), 1, 5);
    const last = Math.max(first, half - w * 0.12);
    const gap = sides > 1 ? clamp((last - first) / (sides - 1), w * 0.14, w * 0.45) : w * 0.3;
    const spare = Math.max(0, room - w);
    return { w, left: (size - w) / 2, top: TOP + spare, spare, u: w / 184, sides, first, gap };
  });
  $effect(() => onspare?.(front.spare));

  /** The cover `d` places from the front one. */
  function place(d: number): Place {
    const a = Math.abs(d),
      side = d < 0 ? -1 : 1;
    const near = Math.min(a, 1),
      far = Math.max(a - 1, 0);
    const { u, w, gap, sides, first } = front;
    // The ends curve down a little; the front one flat at its place, all of it seen.
    const drop = (near * 12 + far * 10) * u;
    // Where it should seem to be, moved out as far again as its depth draws it in.
    const z = -(near * 150 + far * 60) * u;
    const perspective = 620 * u;
    const x = (near * first + far * gap) * ((perspective - z) / perspective);
    return {
      t: `translateX(${side * x}px) translateY(${drop}px) translateZ(${z}px) rotateZ(${side * (near * 3 + far * 2)}deg) rotateY(${-side * (near * 50 + far * 4)}deg)`,
      f: 1 - near * 0.25 - far * (0.35 / sides),
      o: a <= sides + 0.6 ? 1 : clamp(sides + 1.2 - a, 0, 1),
      z: Math.round(100 - a * 10),
      w,
    };
  }
  // Coming in: each cover from its side's edge (the front one only fading in), the outer ones first; then as ever.
  const ENTER_MS = 650;
  const STAGGER_MS = 70;
  let entering = $state(untrack(() => enter) && !matchMedia('(prefers-reduced-motion: reduce)').matches);
  let entered = $state(!untrack(() => entering));
  $effect(() => {
    if (!entering || !measured) return;
    let t = 0;
    const r = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        entering = false;
        t = window.setTimeout(() => (entered = true), ENTER_MS + STAGGER_MS * 6);
      }),
    );
    return () => {
      cancelAnimationFrame(r);
      clearTimeout(t);
    };
  });
  const items = $derived.by(() => {
    if (!n || !measured) return [];
    if (single) return [{ key: 0, src: covers[0], marked: !!marked?.[0], ...place(0) }];
    const base = Math.floor(pos);
    const out: (Place & { key: number; src: string; marked: boolean })[] = [];
    for (let k = base - front.sides - 1; k <= base + front.sides + 2; k++) {
      const p = place(k - pos);
      if (p.o > 0.01) out.push({ key: k, src: covers[wrap(k)], marked: !!marked?.[wrap(k)], ...p });
    }
    return out;
  });

  /** The front cover's place in `covers`. */
  const at = $derived(n ? wrap(Math.round(pos)) : 0);
  $effect(() => {
    if (n) onfront?.(at);
  });
  // The next few on each side, fetched ahead so a quick flick does not show them blank.
  $effect(() => {
    if (!n) return;
    const reach = front.sides + 3;
    for (let k = -reach; k <= reach; k++) new Image().src = covers[wrap(Math.round(pos) + k)];
  });

  // A drag sideways moves through them; let go, the nearest comes to the front. A tap: one on, or (`onopen`) the front
  // one opens and one beside it comes to the front.
  // A gesture that goes up or down is the page's scroll, never a tap (the owner, 2026-10-23: "opening while scrolling
  // really fucks me up"); the browser takes it over and cancels it here.
  // A press held still (lib/longpress's time) or a right-click is the front cover's menu, and then no tap.
  let start: { x: number; y: number; pos: number; moved: boolean; scroll: boolean; menu: boolean } | null = null;
  let tapX = 0;
  let hold: ReturnType<typeof setTimeout> | undefined;
  let root = $state<HTMLElement>();
  const HOLD_MS = 500;
  function menuNow() {
    if (!onmenu || !n || (start && (start.moved || start.scroll || start.menu))) return;
    if (start) start.menu = true;
    dragging = false;
    onmenu(at, root?.getBoundingClientRect() ?? new DOMRect());
  }
  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    start = { x: e.clientX, y: e.clientY, pos, moved: false, scroll: false, menu: false };
    tapX = e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
    clearTimeout(hold);
    if (onmenu && e.button === 0) hold = setTimeout(menuNow, HOLD_MS);
  }
  function context(e: MouseEvent) {
    if (!onmenu) return;
    e.preventDefault();
    menuNow();
  }
  function move(e: PointerEvent) {
    if (!start || start.scroll) return;
    const dx = e.clientX - start.x,
      dy = e.clientY - start.y;
    if (!start.moved) {
      if (Math.abs(dy) >= 6 && Math.abs(dy) > Math.abs(dx)) {
        start.scroll = true;
        clearTimeout(hold);
        return;
      }
      if (Math.abs(dx) < 6) return;
    }
    clearTimeout(hold);
    if (start.menu) return;
    start.moved = true;
    if (single) return;
    dragging = true;
    pos = start.pos - dx / (front.w * 0.42);
  }
  function cancel() {
    clearTimeout(hold);
    start = null;
    dragging = false;
    pos = Math.round(pos);
  }
  function up() {
    clearTimeout(hold);
    if (!start) return;
    if (start.scroll) return cancel();
    if (start.menu) {
      start = null;
      return;
    }
    const tap = !start.moved;
    start = null;
    dragging = false;
    if (!tap) {
      pos = Math.round(pos);
      return;
    }
    if (single) return void (onopen && tapX >= front.left && tapX <= front.left + front.w && onopen(0));
    if (onopen) {
      if (tapX < front.left) pos = Math.round(pos) - 1;
      else if (tapX > front.left + front.w) pos = Math.round(pos) + 1;
      else onopen(at);
      return;
    }
    pos = Math.round(pos) + 1;
  }
  function key(e: KeyboardEvent) {
    if (single && e.key !== 'Enter') return;
    if (e.key === 'ArrowLeft') pos = Math.round(pos) - 1;
    else if (e.key === 'ArrowRight') pos = Math.round(pos) + 1;
    else if (e.key === 'Enter' && onopen) onopen(at);
    else return;
    e.preventDefault();
  }
</script>

<div
  class="flow"
  bind:this={root}
  bind:clientWidth={size}
  bind:clientHeight={height}
  class:fade
  style:bottom={bleed ? `${-bleed}px` : null}
  style:perspective="{620 * front.u}px"
  role="listbox"
  aria-label="Albums"
  tabindex="0"
  onkeydown={key}
  onpointerdown={down}
  oncontextmenu={context}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={cancel}
>
  {#each items as c (c.key)}
    <span
      class="rec"
      class:still={dragging || !settled}
      class:marked={c.marked}
      style:top="{front.top}px"
      style:left="{front.left}px"
      style:width="{c.w}px"
      style:height="{c.w}px"
      style:transform={entering && c.key !== Math.round(pos) ? `translateX(${(c.key < pos ? -1 : 1) * size}px) ${c.t}` : c.t}
      style:filter="brightness({c.f})"
      style:opacity={entering || !ready[c.src] ? 0 : c.o}
      style:transition-duration={entered ? null : `${ENTER_MS}ms`}
      style:transition-delay={entered ? null : `${Math.max(0, front.sides + 1 - Math.abs(c.key - Math.round(pos))) * STAGGER_MS}ms`}
      class:veiled={!!frontLayer && c.key === Math.round(pos)}
      style:z-index={c.z}><Cover src={c.src} lazy={false} onready={() => (ready[c.src] = true)} />{#if frontLayer && c.key === Math.round(pos)}{@render frontLayer()}{/if}</span
    >
  {/each}
</div>

<style>
  .flow {
    position: absolute;
    inset: 0;
    overflow: hidden;
    touch-action: pan-y;
    user-select: none;
    cursor: grab;
  }

  /* Its sides fade out (the owner, 2026-10-23: proper fades, no hard cut-offs). */
  .flow.fade {
    --edge: min(12%, 96px);
    -webkit-mask-image: linear-gradient(to right, transparent, #000 var(--edge), #000 calc(100% - var(--edge)), transparent);
    mask-image: linear-gradient(to right, transparent, #000 var(--edge), #000 calc(100% - var(--edge)), transparent);
  }

  .rec {
    position: absolute;
    display: block;
    overflow: hidden;
    border-radius: 4px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 55%);
    transition:
      transform 0.45s cubic-bezier(0.2, 0.8, 0.2, 1),
      top 0.45s cubic-bezier(0.2, 0.8, 0.2, 1),
      filter 0.45s,
      opacity 0.45s;
  }

  /* Under the front cover's details: blurred, as the player's cover under its overview; its text sized by its width. */
  .rec {
    container-type: inline-size;
  }

  .rec.veiled > :global(.cover) {
    filter: blur(14px) saturate(1.2);
  }

  .rec.marked {
    outline: 3px solid var(--accent);
    outline-offset: 3px;
  }

  .rec.still {
    transition: none;
  }
</style>
