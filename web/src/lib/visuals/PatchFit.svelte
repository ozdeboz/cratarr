<script lang="ts">
  // Keeps the whole patch graph in view (inside PatchGraph's SvelteFlow) each time its layout settles, until the
  // viewer pans, zooms or moves something.
  import { useSvelteFlow } from '@xyflow/svelte';

  let { layout, active }: { layout: string; active: boolean } = $props();
  const flow = useSvelteFlow();

  $effect(() => {
    void layout;
    if (!active) return;
    const frame = requestAnimationFrame(() => void flow.fitView({ padding: 0.06 }));
    return () => cancelAnimationFrame(frame);
  });
</script>
