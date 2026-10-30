<script lang="ts">
  // A Sounds like crate's curve (the owner, 2026-10-23): how alike its closest tracks are to the seed, closest first
  // (0 two random tracks, 100 the same sound), on a log scale of their places so the closest get room; the crate ends
  // at the cut, the tracks after it dimmed. The curve is the control: the handle sits on it and is dragged along it
  // (or moved with the arrow keys); set on release.
  let {
    curve,
    count,
    fewest = 10,
    onchange,
    onpreview,
  }: {
    /** How alike the closest tracks are, closest first (the server's /api/like-curve). */
    curve: readonly number[];
    count: number;
    fewest?: number;
    onchange: (count: number) => void;
    /** Where the handle is while it is dragged (null: let go), so what depends on it can follow. */
    onpreview?: (count: number | null) => void;
  } = $props();

  const W = 300;
  const H = 96;
  /** The drawing's margins: on the left the room the scale's labels take ("100%"), on the right what the handle needs
   *  (the owner, 2026-10-25: the labels were cut off, room to spare on the right). */
  const LEFT = 30;
  const RIGHT = 8;
  const TOP = 8;
  /** The handle's radius (the owner, 2026-10-25: smaller). */
  const KNOB = 6;
  const most = $derived(Math.max(fewest, curve.length));
  const x = (rank: number) => (Math.log(rank) / Math.log(most)) * W;
  /** The scale's foot (the owner): the curve's lowest point never above the bottom quarter, at 0 only when it gets
   *  there; the top is 100% (the seed). */
  const floor = $derived(Math.max(0, (Math.min(...curve) - 25) / 0.75));
  const y = (alike: number) => H - ((alike - floor) / (100 - floor || 1)) * H;
  const rankAt = (px: number) => Math.round(Math.exp((Math.min(W, Math.max(0, px)) / W) * Math.log(most)));
  const clampCount = (n: number) => Math.min(most, Math.max(fewest, n));

  let drag = $state<number | null>(null);
  const at = $derived(clampCount(drag ?? count));
  const points = $derived(curve.map((a, i) => `${x(i + 1).toFixed(1)},${y(a).toFixed(1)}`).join(' '));
  const handle = $derived({ cx: x(at), cy: y(curve[at - 1] ?? 0) });
  /** Round numbers while dragging: a handle at 213 reads as noise. */
  const tidy = (n: number) => (n < 100 ? Math.round(n / 5) * 5 : n < 400 ? Math.round(n / 10) * 10 : Math.round(n / 50) * 50);

  let svg = $state<SVGSVGElement>();
  function rankOf(e: PointerEvent) {
    const r = svg!.getBoundingClientRect();
    // The drawing's own units: the view box is wider than W by its margins.
    const px = ((e.clientX - r.left) / r.width) * (W + LEFT + RIGHT) - LEFT;
    return clampCount(tidy(rankAt(px)));
  }
  // As any slider (the owner, 2026-10-25): the handle moves when it is grabbed, or anywhere after a long press; a tap
  // or a swipe elsewhere leaves it (the page scrolls over it).
  const GRAB_PX = 24;
  const HOLD_MS = 400;
  const STILL_PX = 8;
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  let pressAt: { x: number; y: number } | null = null;
  /** Whether the pointer is on the handle (within GRAB_PX of it on screen). */
  function onHandle(e: PointerEvent) {
    const r = svg!.getBoundingClientRect();
    const hx = r.left + ((handle.cx + LEFT) / (W + LEFT + RIGHT)) * r.width;
    const hy = r.top + ((handle.cy + TOP) / (H + TOP + 18)) * r.height;
    return Math.hypot(e.clientX - hx, e.clientY - hy) <= GRAB_PX;
  }
  function grab(e: PointerEvent) {
    svg?.setPointerCapture(e.pointerId);
    drag = rankOf(e);
    onpreview?.(drag);
  }
  function down(e: PointerEvent) {
    if (onHandle(e)) {
      e.preventDefault();
      return grab(e);
    }
    pressAt = { x: e.clientX, y: e.clientY };
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      if (!pressAt) return;
      navigator.vibrate?.(8);
      grab(e);
    }, HOLD_MS);
  }
  function move(e: PointerEvent) {
    if (drag === null) {
      if (pressAt && Math.hypot(e.clientX - pressAt.x, e.clientY - pressAt.y) > STILL_PX) cancelHold();
      return;
    }
    const n = rankOf(e);
    if (n !== drag) onpreview?.(n);
    drag = n;
  }
  function cancelHold() {
    clearTimeout(holdTimer);
    pressAt = null;
  }
  $effect(() => () => clearTimeout(holdTimer));
  function up() {
    cancelHold();
    if (drag === null) return;
    const n = drag;
    drag = null;
    onpreview?.(null);
    if (n !== count) onchange(n);
  }
  function key(e: KeyboardEvent) {
    const d = e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : 0;
    if (!d) return;
    e.preventDefault();
    const steps = [10, 20, 30, 50, 75, 100, 150, 200, 300, 400, 500, 750, 1000].filter((s) => s <= most);
    const next = d > 0 ? steps.find((s) => s > count) : [...steps].reverse().find((s) => s < count);
    if (next) onchange(next);
  }
</script>

<svg
  bind:this={svg}
  class="curve"
  viewBox="{-LEFT} {-TOP} {W + LEFT + RIGHT} {H + TOP + 18}"
  role="slider"
  tabindex="0"
  aria-label="How many tracks"
  aria-valuemin={fewest}
  aria-valuemax={most}
  aria-valuenow={at}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
>
  <!-- Fewer than the fewest cannot be. -->
  <rect x="0" y="0" width={x(fewest)} height={H} class="none" />
  <polygon points="0,{H} {points} {x(curve.length)},{H}" class="area" />
  <polyline {points} class="line" />
  <rect x={handle.cx} y={-TOP} width={Math.max(0, W - handle.cx + RIGHT)} height={H + TOP} class="after" />
  <line x1={handle.cx} x2={handle.cx} y1="0" y2={H} class="cut" />
  <circle cx={handle.cx} cy={handle.cy} r={KNOB} class="knob" />
  <text x="0" y={H + 14}>1</text>
  <text x={x(fewest)} y={H + 14} text-anchor="middle">{fewest}</text>
  <text x={x(100)} y={H + 14} text-anchor="middle">100</text>
  <text x={W} y={H + 14} text-anchor="end">{most.toLocaleString()}</text>
  <text x="-4" y="7" text-anchor="end">100%</text>
  <text x="-4" y={H} text-anchor="end">{Math.round(floor)}%</text>
</svg>

<style>
  .curve {
    display: block;
    width: 100%;
    overflow: visible;
    /* Swiped over, the page scrolls; a grabbed handle (or a long press) drags. */
    touch-action: pan-y;
    /* A drag selects no text. */
    user-select: none;
  }

  .curve:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
    border-radius: 8px;
  }

  .none {
    fill: color-mix(in srgb, var(--text) 4%, transparent);
  }

  .area {
    fill: color-mix(in srgb, var(--accent) 28%, transparent);
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }

  /* After the cut: not in the crate. */
  .after {
    fill: var(--surface);
    opacity: 0.62;
  }

  .cut {
    stroke: var(--text);
    stroke-width: 2;
  }

  .knob {
    cursor: ew-resize;
    fill: var(--text);
    stroke: rgb(0 0 0 / 0.35);
  }

  text {
    fill: var(--muted);
    font-size: 10px;
  }
</style>
