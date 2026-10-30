<script lang="ts">
  // A picture's (or light's) wire on the patch graph: its ELK route (`data.route`, square), else Svelte Flow's bezier or
  // (`data.step`) its steps, from jack centre to jack centre (patchgraph.ts toJack: half a jack in from the edge point
  // Svelte Flow gives), a plug at each end.
  import { getContext } from 'svelte';
  import { BaseEdge, getBezierPath, getSmoothStepPath, type EdgeProps } from '@xyflow/svelte';
  import { GRAPH, JACK, routePath, toJack, type CableData, type GraphActions } from './patchgraph';

  let { sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, style, label }: EdgeProps = $props();
  const d = $derived((data ?? { amount: '', wire: 'picture' }) as CableData);
  const a = $derived(toJack(sourceX, sourceY, sourcePosition));
  const b = $derived(toJack(targetX, targetY, targetPosition));
  const graph = getContext<GraphActions>(GRAPH);
  const [path, labelX, labelY] = $derived.by(() => {
    if (d.route) {
      const [p, mid] = routePath(a, d.route, b, graph.portrait());
      return [p, mid.x, mid.y] as const;
    }
    const ends = { sourceX: a.x, sourceY: a.y, sourcePosition, targetX: b.x, targetY: b.y, targetPosition };
    return d.step ? getSmoothStepPath({ ...ends, offset: JACK + 10 }) : getBezierPath(ends);
  });
</script>

<BaseEdge {path} {style} {label} {labelX} {labelY} interactionWidth={10} />
<circle cx={a.x} cy={a.y} r="3.4" class="plug w-{d.wire}" />
<circle cx={b.x} cy={b.y} r="3.4" class="plug w-{d.wire}" />

<style>
  .plug {
    fill: var(--text);
    stroke: var(--patch-bg);
    stroke-width: 1.2;
    pointer-events: none;
  }
  .plug.w-light {
    fill: var(--wire-light);
  }
</style>
