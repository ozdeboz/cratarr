<script lang="ts">
  // A Thumbnail (face.ts): a picture module's output, small and live. Its canvas is registered with the graph's
  // thumbnails while it is drawn (`thumbs`); the graph has the renderer draw one of them a frame, in turn, and only
  // while the graph shows. Until its first drawing, a dark tile.
  import type { Thumbs } from './face';

  let { module, thumbs, width = 130, height = 73 }: { module: string; thumbs?: Thumbs | null; width?: number; height?: number } = $props();

  let canvas = $state<HTMLCanvasElement>();
  $effect(() => {
    if (!canvas || !thumbs) return;
    return thumbs.add(module, canvas);
  });
</script>

<canvas class="thumb" bind:this={canvas} {width} {height} aria-hidden="true"></canvas>

<style>
  .thumb {
    display: block;
    width: 100%;
    max-width: var(--thumb-w, 130px);
    aspect-ratio: 16 / 9;
    border-radius: 6px;
    background: #050507;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text) 10%, transparent);
  }
</style>
