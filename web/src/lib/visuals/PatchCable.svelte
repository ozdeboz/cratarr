<script lang="ts">
  // A modulation cable on the patch graph: dashed, hanging with a slight sag (patchgraph.ts `saggingPath`, after VCV
  // Rack's cables); its amount shown while it or a node at its ends is in focus. A tap opens it (PatchGraph); a long
  // press or a right click (lib/longpress), its menu. Rerouted, the end nearer the press gets a grip to drag onto another
  // knob or output (Svelte Flow's reconnection). Its hit path is wider than it looks, wider still under a finger.
  // Its wire is its type's (patchgraph.ts WIRES, as CSS variables in `style`): the sound's a double line (its middle
  // drawn out); a band's six at once marked "· 6". A port's normal (`normal`) is the line alone, faint, never tapped.
  // It runs from jack centre to jack centre and ends in a plug in each, so no dash stops short of a socket.
  import { getContext } from 'svelte';
  import { BaseEdge, EdgeLabel, EdgeReconnectAnchor, useSvelteFlow, type EdgeProps } from '@xyflow/svelte';
  import { longpress } from '../longpress';
  import Icon from '../Icon.svelte';
  import { cableLabel, GRAPH, nearestEnd, routePath, saggingPath, toJack, WIRES, type CableData, type GraphActions } from './patchgraph';

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, selected, style }: EdgeProps = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const flow = useSvelteFlow();
  // Over the nodes, its hit line stays near the line itself, so a knob under it still turns.
  const HIT = typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches ? 18 : 10;

  const d = $derived((data ?? { amount: '', wire: 'value' }) as CableData);
  // From jack centre to jack centre (toJack), with VCV Rack's sag between.
  const a = $derived(toJack(sourceX, sourceY, sourcePosition));
  const b = $derived(toJack(targetX, targetY, targetPosition));
  // Or its ELK route, where the graph routes cables so.
  const [path, mid] = $derived(d.route && graph.routeCables() ? routePath(a, d.route, b, graph.portrait()) : saggingPath(a, b));
  const label = $derived(cableLabel(d, !!(selected || d.lit)));

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

<BaseEdge {path} {style} interactionWidth={0} class={reconnecting ? 'cable-path held' : 'cable-path'} />
{#if WIRES[d.wire]?.double}<path d={path} class="gap" />{/if}
<!-- A plug at each end, over its jack's hole: the cable plainly in it, whatever its dash. -->
<circle cx={a.x} cy={a.y} r="3.4" class="plug w-{d.wire}" class:normal={d.normal} />
<circle cx={b.x} cy={b.y} r="3.4" class="plug w-{d.wire}" class:normal={d.normal} />
{#if !d.normal}
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
{#if (label || d.pinned) && !reconnecting}
  <EdgeLabel x={mid.x} y={mid.y} class="cable-label nodrag nopan" onclick={() => graph.openCable(id)}>
    <span class:selected class="w-{d.wire}">{#if d.pinned}<i class="pinned" title="Pinned"><Icon name="pin" size={11} /></i>{/if}{label}</span>
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
{/if}

<style>
  .plug {
    fill: var(--wire-value);
    stroke: var(--patch-bg);
    stroke-width: 1.2;
    pointer-events: none;
  }
  .plug.w-trigger {
    fill: var(--wire-trigger);
  }
  .plug.w-audio {
    fill: var(--wire-audio);
  }
  .plug.w-points {
    fill: var(--wire-points);
  }
  .plug.w-flow {
    fill: var(--wire-flow);
  }
  .plug.normal {
    opacity: 0.45;
  }
  .gap {
    fill: none;
    stroke: var(--patch-bg);
    stroke-width: 1.6px;
    pointer-events: none;
  }
  span {
    font-size: 11.5px;
    color: var(--patch-mod);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  span.w-trigger {
    color: var(--wire-trigger);
  }
  span.w-audio {
    color: var(--wire-audio);
  }
  span.w-flow {
    color: var(--wire-flow);
  }
  span.selected {
    color: var(--text);
  }
  .pinned {
    display: inline-block;
    margin-right: 2px;
    color: var(--accent-ink);
    line-height: 0;
    vertical-align: -1px;
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
