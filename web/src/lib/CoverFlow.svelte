<script lang="ts">
  // Albums to flip through, a coverflow (the owner, 2026-10-23: "more representative of digging through albums in a
  // visual way"): a crate's header, and the Library's Flow layout. Curved sideways, the front one facing, the ends
  // curving down a little; a drag sideways moves through all of them, a tap goes one on (with `onopen`: a tap on the
  // front one opens it, one beside it comes to the front), the arrow keys too, and they go round. It lays itself out
  // from the room it has: the front cover as large as the height allows between `top` and `foot`, the covers beside it
  // spread over the width (off its edges where it is narrow), the sides fading out. Only the covers in sight are made,
  // and the next few are fetched ahead.
  import Cover from './Cover.svelte';

  let {
    covers,
    top: TOP = 64,
    foot: FOOT = 72,
    onopen,
    onfront,
    onspare,
  }: {
    covers: string[];
    /** Room kept free along the top (a header's buttons) and over the foot (the fade under its action bar). */
    top?: number;
    foot?: number;
    /** A tap on the front cover: its place in `covers`. */
    onopen?: (i: number) => void;
    /** The front cover changed: its place. */
    onfront?: (i: number) => void;
    /** The height the covers leave unused (they sit at the foot): what holds them can centre them with what follows. */
    onspare?: (px: number) => void;
  } = $props();

  let size = $state(340);
  let height = $state(340);
  /** The front cover's place in `covers`, between whole places while dragged. */
  let pos = $state(0);
  let dragging = $state(false);
  const n = $derived(covers.length);
  const wrap = (i: number) => ((i % n) + n) % n;
  const clamp = (x: number, a: number, b: number) => Math.max(a, Math.min(b, x));

  type Place = { t: string; f: number; o: number; z: number; w: number };
  /** The layout from the room there is (the owner, 2026-10-23: responsive, the space used). */
  const front = $derived.by(() => {
    const room = height - TOP - FOOT - 8;
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
  const items = $derived.by(() => {
    if (!n) return [];
    const base = Math.floor(pos);
    const out: (Place & { key: number; src: string })[] = [];
    for (let k = base - front.sides - 1; k <= base + front.sides + 2; k++) {
      const p = place(k - pos);
      if (p.o > 0.01) out.push({ key: k, src: covers[wrap(k)], ...p });
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
  let start: { x: number; pos: number; moved: boolean } | null = null;
  let tapX = 0;
  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    start = { x: e.clientX, pos, moved: false };
    tapX = e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
  }
  function move(e: PointerEvent) {
    if (!start) return;
    const dx = e.clientX - start.x;
    if (!start.moved && Math.abs(dx) < 6) return;
    start.moved = true;
    dragging = true;
    pos = start.pos - dx / (front.w * 0.42);
  }
  function up() {
    if (!start) return;
    const tap = !start.moved;
    start = null;
    dragging = false;
    if (!tap) {
      pos = Math.round(pos);
      return;
    }
    if (onopen) {
      if (tapX < front.left) pos = Math.round(pos) - 1;
      else if (tapX > front.left + front.w) pos = Math.round(pos) + 1;
      else onopen(at);
      return;
    }
    pos = Math.round(pos) + 1;
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') pos = Math.round(pos) - 1;
    else if (e.key === 'ArrowRight') pos = Math.round(pos) + 1;
    else if (e.key === 'Enter' && onopen) onopen(at);
    else return;
    e.preventDefault();
  }
</script>

<div
  class="flow"
  bind:clientWidth={size}
  bind:clientHeight={height}
  style:perspective="{620 * front.u}px"
  role="listbox"
  aria-label="Albums"
  tabindex="0"
  onkeydown={key}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
>
  {#each items as c (c.key)}
    <span
      class="rec"
      class:still={dragging}
      style:top="{front.top}px"
      style:left="{front.left}px"
      style:width="{c.w}px"
      style:height="{c.w}px"
      style:transform={c.t}
      style:filter="brightness({c.f})"
      style:opacity={c.o}
      style:z-index={c.z}><Cover src={c.src} lazy={false} /></span
    >
  {/each}
</div>

<style>
  .flow {
    position: absolute;
    inset: 0;
    overflow: hidden;
    /* Its sides fade out (the owner, 2026-10-23: proper fades, no hard cut-offs). */
    --edge: min(12%, 96px);
    -webkit-mask-image: linear-gradient(to right, transparent, #000 var(--edge), #000 calc(100% - var(--edge)), transparent);
    mask-image: linear-gradient(to right, transparent, #000 var(--edge), #000 calc(100% - var(--edge)), transparent);
    touch-action: pan-y;
    user-select: none;
    cursor: grab;
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

  .rec.still {
    transition: none;
  }
</style>
