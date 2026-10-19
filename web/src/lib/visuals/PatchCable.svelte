<script lang="ts">
  // A modulation cable on the patch graph: dashed, its amount on it; selected, a × unplugs it.
  import { getContext } from 'svelte';
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps } from '@xyflow/svelte';
  import { GRAPH, type GraphActions } from './patchgraph';

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, selected }: EdgeProps = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const [path, labelX, labelY] = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, curvature: 0.4 }));
</script>

<BaseEdge {path} interactionWidth={16} class="cable-path" />
<EdgeLabel x={labelX} y={labelY} selectEdgeOnClick class="cable-label nodrag nopan">
  <span class:selected>{(data as { amount: string } | undefined)?.amount ?? ''}</span>
  {#if selected}<button onclick={() => graph.disconnect(id)} aria-label="Unplug">×</button>{/if}
</EdgeLabel>

<style>
  span {
    font-size: 11.5px;
    color: var(--patch-mod);
    font-variant-numeric: tabular-nums;
  }
  span.selected {
    color: var(--text);
  }
  button {
    margin-left: 4px;
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: 50%;
    background: var(--patch-mod);
    color: var(--bg);
    font: 600 12px/18px system-ui, sans-serif;
    cursor: pointer;
    padding: 0;
  }
</style>
