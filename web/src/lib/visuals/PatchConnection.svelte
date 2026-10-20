<script lang="ts">
  // The cable being drawn or rerouted on the patch graph (Svelte Flow's connection line): dashed, with the same sag as
  // the cables (patchgraph.ts `saggingPath`, after VCV Rack's).
  import { useConnection } from '@xyflow/svelte';
  import { saggingPath } from './patchgraph';

  const connection = useConnection();
  const path = $derived.by(() => {
    const c = connection.current;
    return c.inProgress ? saggingPath(c.from, c.to)[0] : '';
  });
</script>

<path d={path} fill="none" class="svelte-flow__connection-path" />
