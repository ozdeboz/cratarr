<script lang="ts">
  // Keeps the whole patch graph in view (inside PatchGraph's SvelteFlow) each time its layout settles, until the
  // viewer pans, zooms or moves something; `padding` keeps it clear of what lies over the canvas (an open sheet, the
  // chips). Reports the zoom as it changes (labels and the far view follow it).
  import { useSvelteFlow, useViewport, type FitViewOptions } from '@xyflow/svelte';

  let { layout, active, padding, onzoom }: { layout: string; active: boolean; padding: FitViewOptions['padding']; onzoom?: (zoom: number) => void } = $props();
  const flow = useSvelteFlow();
  const viewport = useViewport();

  $effect(() => {
    void layout;
    void JSON.stringify(padding);
    if (!active) return;
    const frame = requestAnimationFrame(() => void flow.fitView({ padding }));
    return () => cancelAnimationFrame(frame);
  });
  $effect(() => onzoom?.(viewport.current.zoom));
</script>
