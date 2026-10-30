<script lang="ts">
  // Opens the patch graph (inside PatchGraph's SvelteFlow) VCV Rack's way each time its layout settles, until the
  // viewer pans, zooms or moves something: at FULL_ZOOM, modules whole, the patch centred where it fits and else
  // started at the view's start (patchgraph.ts openView): panned to, never shrunk. `pad` keeps it clear of what lies over
  // the canvas (the top panels, the chips, an open sheet). Reports the zoom as it changes (the overview below it).
  // Once the fonts are in, every node's handles are measured again.
  import { useNodes, useSvelteFlow, useUpdateNodeInternals, useViewport } from '@xyflow/svelte';
  import { firstModule, openView, type NodeData, type Pad, type Size } from './patchgraph';

  let { layout, active, pad, view, portrait = false, onzoom, onview }: { layout: string; active: boolean; pad: Pad; view: Size; portrait?: boolean; onzoom?: (zoom: number) => void; onview?: (v: { x: number; y: number; zoom: number }) => void } = $props();
  const flow = useSvelteFlow();
  const viewport = useViewport();
  const all = useNodes();

  $effect(() => {
    void layout;
    void JSON.stringify(pad);
    if (!active || view.width <= 0 || view.height <= 0) return;
    const frame = requestAnimationFrame(() => {
      const top = all.current.filter((n) => !n.parentId);
      if (top.length) void flow.setViewport(openView(flow.getNodesBounds(top), view, pad, undefined, firstModule(top as unknown as { position: { x: number; y: number }; data: NodeData }[], portrait)));
    });
    return () => cancelAnimationFrame(frame);
  });
  $effect(() => onzoom?.(viewport.current.zoom));
  // The viewport as it is (where a module added is put: the view's centre).
  $effect(() => onview?.({ ...viewport.current }));

  const update = useUpdateNodeInternals();
  $effect(() => {
    let live = true;
    void document.fonts?.ready.then(() => live && update(all.current.map((n) => n.id)));
    return () => (live = false);
  });
</script>
