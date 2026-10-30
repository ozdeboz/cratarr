<script lang="ts">
  // The one preview (the owner, 71.1: "previews only for the selected module", to save the GPU): a small popover with
  // the picked module's output, shown only while exactly one module with a preview is picked or was last tapped. A
  // picture's (or light's) through the renderer's thumbnail (the module observed, read back a little late; made anew
  // for each module, so it never shows another's), or a line saying why it has none; a points output (a Grid's)
  // drawn as dots on a 2D canvas from the engine's frame, no GPU pass.
  import type { LiveFrame } from '../faces';
  import { pointsOf, type Thumbs } from './face';

  let { module, label, kind, live, thumbs }: { module: string; label: string; kind: 'picture' | 'points'; live: LiveFrame | null; thumbs: Thumbs | null } = $props();

  const W = 192;
  const H = 108;
  let canvas = $state<HTMLCanvasElement>();
  let why = $state<string | null>(null);
  // A picture: its canvas the one thumbnail drawn, while it shows; cleared where it has none.
  $effect(() => {
    if (kind !== 'picture' || !canvas || !thumbs) return;
    const c = canvas;
    return thumbs.add(module, c, (w) => {
      if (w && w !== why) c.getContext('2d')?.clearRect(0, 0, c.width, c.height);
      why = w;
    });
  });
  // Points: dots, each frame.
  $effect(() => {
    if (kind !== 'points' || !canvas) return;
    const pts = pointsOf(live, module);
    const g = canvas.getContext('2d');
    if (!g) return;
    g.fillStyle = '#050507';
    g.fillRect(0, 0, W, H);
    g.fillStyle = getComputedStyle(canvas).getPropertyValue('--dot').trim() || '#e98fc8';
    // The square of points centred in the wider canvas.
    const side = H - 8;
    const x0 = (W - side) / 2;
    for (const p of pts ?? []) {
      g.beginPath();
      g.arc(x0 + p.x * side, 4 + (1 - p.y) * side, 1.8, 0, Math.PI * 2);
      g.fill();
    }
  });
</script>

<div class="preview" role="img" aria-label="{label}: preview">
  <span class="name">{label}</span>
  <div class="frame">
    <canvas bind:this={canvas} width={W} height={H}></canvas>
    {#if why}<span class="why">{why}</span>{/if}
  </div>
</div>

<style>
  .preview {
    display: grid;
    gap: 4px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--float-bg, var(--surface));
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
  }
  .name {
    color: var(--muted);
    font-size: 12px;
  }
  .frame {
    position: relative;
  }
  .why {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 8px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }
  canvas {
    --dot: var(--wire-points, #e98fc8);
    display: block;
    width: 192px;
    height: 108px;
    border-radius: 6px;
    background: #050507;
  }
</style>
