<script lang="ts">
  // A modulation cable on the patch graph: dashed, hanging with a slight sag (patchgraph.ts `saggingPath`, after VCV
  // Rack's cables); its amount shown while it or a node at its ends is in focus. A tap opens it (PatchGraph); a long
  // press or a right click (lib/longpress), its menu. Rerouted, the end nearer the press gets a grip to drag onto another
  // knob or output (Svelte Flow's reconnection). Its hit path is wider than it looks, wider still under a finger.
  import { getContext } from 'svelte';
  import { BaseEdge, EdgeLabel, EdgeReconnectAnchor, useSvelteFlow, type EdgeProps } from '@xyflow/svelte';
  import { longpress } from '../longpress';
  import { GRAPH, nearestEnd, saggingPath, type GraphActions } from './patchgraph';

  let { id, sourceX, sourceY, targetX, targetY, data, selected }: EdgeProps = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const flow = useSvelteFlow();
  const HIT = typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches ? 28 : 16;

  const [path, mid] = $derived(saggingPath({ x: sourceX, y: sourceY }, { x: targetX, y: targetY }));
  const d = $derived((data ?? {}) as { amount?: string; lit?: boolean });

  // Where the last press on it was (the long press's), for the end to reroute.
  let pressed = { x: 0, y: 0 };
  const reroute = $derived(graph.rerouting());
  const end = $derived.by(() => {
    if (reroute?.id !== id) return null;
    const at = flow.screenToFlowPosition(reroute.at);
    return nearestEnd(at, { x: sourceX, y: sourceY }, { x: targetX, y: targetY });
  });
  let reconnecting = $state(false);
</script>

<BaseEdge {path} interactionWidth={0} class={reconnecting ? 'cable-path held' : 'cable-path'} />
<path
  d={path}
  class="svelte-flow__edge-interaction"
  fill="none"
  stroke="transparent"
  stroke-width={HIT}
  style="pointer-events: stroke"
  role="presentation"
  onpointerdown={(e) => (pressed = { x: e.clientX, y: e.clientY })}
  use:longpress={() => graph.cableMenu(id, pressed)}
/>
{#if (selected || d.lit) && !reconnecting}
  <EdgeLabel x={mid.x} y={mid.y} class="cable-label nodrag nopan" onclick={() => graph.openCable(id)}>
    <span class:selected>{d.amount ?? ''}</span>
  </EdgeLabel>
{/if}
{#if end}
  <EdgeReconnectAnchor
    type={end === 'from' ? 'source' : 'target'}
    position={end === 'from' ? { x: sourceX, y: sourceY } : { x: targetX, y: targetY }}
    size={HIT + 8}
    bind:reconnecting
    aria-label="Drag to reroute"
  >
    <i class="grip"></i>
  </EdgeReconnectAnchor>
{/if}

<style>
  span {
    font-size: 11.5px;
    color: var(--patch-mod);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  span.selected {
    color: var(--text);
  }
  .grip {
    display: block;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    border: 2px solid var(--patch-mod);
    border-radius: 50%;
    background: color-mix(in srgb, var(--patch-mod) 25%, transparent);
    cursor: grab;
  }
</style>
