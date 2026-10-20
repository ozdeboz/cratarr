<script lang="ts">
  // Keeps the whole patch graph in view (inside PatchGraph's SvelteFlow) each time its layout settles, until the
  // viewer pans, zooms or moves something; `padding` keeps it clear of what lies over the canvas (an open sheet, the
  // chips). Reports the zoom as it changes (labels and the far view follow it). Once the fonts are in, every node's
  // handles are measured again.
  import { useNodes, useSvelteFlow, useUpdateNodeInternals, useViewport, type FitViewOptions } from '@xyflow/svelte';

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

  // Once the fonts are in, text may have moved the handles: every node's measured again.
  const update = useUpdateNodeInternals();
  const all = useNodes();
  $effect(() => {
    let live = true;
    void document.fonts?.ready.then(() => live && update(all.current.map((n) => n.id)));
    return () => (live = false);
  });
</script>
