<script lang="ts">
  // A node of the patch graph (patchgraph.ts): a module (its knobs, its picture in and out), a modulator or a signal
  // (an output to cable from), the blend, the screen, or a box around a layer or the loop; a layer that is off as one
  // row ("+ bring back"); "+ Add layer", "+ Add modulator". Tapping its title, or double-clicking it, opens its page; a
  // knob tapped, what moves it; a long press (or right click) on a module that can go, its menu (lib/longpress).
  // Each handle by its port's type (0057 §1: the wire's colour; a trigger's a diamond, the sound's a ring); a modulator
  // with several outputs, a row of jacks (the Clock's Beat, Bar and Phrase blink); a control source, its face.
  import { getContext } from 'svelte';
  import { Handle, Position, useUpdateNodeInternals } from '@xyflow/svelte';
  import { longpress } from '../longpress';
  import PatchKnob from './PatchKnob.svelte';
  import PatchFace from './PatchFace.svelte';
  import Icon from '../Icon.svelte';
  import { clockLights } from './faces';
  import { GRAPH, knobHandle, MOD_OUT, nextPort, polyTurned, type GraphActions, type KnobView, type NodeData, type Side } from './patchgraph';

  let { id, data, selected }: { id: string; data: NodeData; selected?: boolean } = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const update = useUpdateNodeInternals();

  const SIDE: Record<Side, Position> = { left: Position.Left, right: Position.Right, top: Position.Top, bottom: Position.Bottom };
  // Picture handles sit on the title's line at the sides.
  const at = (s: Side) => (s === 'left' || s === 'right' ? 'top: 19px' : undefined);
  // A polyphonic knob turned as one: every channel as far.
  function turn(k: KnobView, v: number) {
    if (!k.channels) return graph.knob(id, k.id, v);
    for (const c of polyTurned(k, v)) graph.knob(id, c.id, c.value);
  }
  // A polyphonic knob opened out to its channels, or one of them back to the one.
  const polyOf = (k: KnobView) => (k.channels ? k.id : /^(.+)\.\d+$/.exec(k.id)?.[1]);
  const pair = (k: KnobView) => `${id}.${polyOf(k)}`;

  // The Clock's lights, from the graph's live frame (only a Clock's node reads it).
  const lights = $derived(data.kind === 'module' && data.face?.kind === 'clock' ? (clockLights(graph.live()?.beat ?? null) as Record<string, number>) : null);

  // The handles move when the knobs, ports or jacks shown change: measured again.
  const shape = $derived(data.kind === 'module' ? data.knobs.map((k) => k.id).join() + '|' + data.ports.map((p) => p.id).join() + '|' + data.outs.length + '|' + (data.face?.lanes ?? 0) : '');
  $effect(() => {
    void shape;
    update(id);
  });
</script>

{#if data.kind === 'box'}
  {@const add = data.add}
  <div class="box" class:dev={data.device} ondblclick={() => graph.open(id)} role="presentation">
    <span class="bl"
      >{data.label}{#if add}<button class="plus nodrag" aria-label="Add to {data.label}" title="Add to {data.label}" onclick={() => graph.adding({ box: add })}>+</button>{/if}{#if data.device}<button
          class="plus nodrag"
          aria-label="Close {data.label}"
          title="Close"
          onclick={() => graph.toggle(id)}>−</button
        ><button class="plus nodrag" aria-label="Into {data.label}" title="Into" onclick={() => graph.enter(id)}>›</button>{/if}</span
    >
  </div>
{:else if data.kind === 'lane'}
  {@const lane = data}
  <div class="lane">
    <span>{lane.label}</span>
    <button class="pin nodrag" class:on={lane.pinned} aria-pressed={lane.pinned} aria-label="Pin {lane.label}" title="Pin" onclick={() => graph.pinCategory(lane.scope, lane.category, !lane.pinned)}
      ><Icon name="pin" size={14} /></button
    >
    <button class="plus nodrag" aria-label="Add to {lane.label}" title="Add" onclick={() => graph.adding({ category: lane.category, scope: lane.scope })}>+</button>
  </div>
{:else if data.kind === 'off'}
  {@const layer = data.layer}
  <div class="node off"><span class="t">{data.label} · off</span><button class="back nodrag" onclick={() => graph.bringBack(layer)}>+ bring back</button></div>
{:else if data.kind === 'add'}
  {@const to = data.to}
  <button class="node add nodrag" onclick={() => graph.adding(to)}>{data.label}</button>
{:else if data.kind === 'signal'}
  <div class="node mod" class:sel={selected} ondblclick={() => graph.open(id)} role="presentation">
    <span class="t">{data.label}</span>
    <span class="sum only">{data.label}</span>
    <Handle type="source" id={MOD_OUT} position={Position.Top} class="out t-{data.type}" />
  </div>
{:else if data.kind === 'module'}
  <div class="node" class:mod={data.modulator} class:src={data.place === 'source'} class:sel={selected} ondblclick={() => graph.open(id)} role="presentation" use:longpress={() => graph.removable(id) && graph.menu(id)}>
    <span class="title">
      <button class="t nodrag" onclick={() => graph.open(id)}>{data.label}<span class="chev">›</span></button>
      {#if data.device}<button class="plus nodrag" aria-label="Open {data.label} here" title="Open here" onclick={() => graph.toggle(id)}>+</button>{/if}
    </span>
    <button class="sum nodrag" tabindex="-1" onclick={() => graph.open(id)}><b>{data.label}</b> · {data.knobs.filter((k) => k.ring).length + data.ports.filter((p) => p.cabled).length} cabled</button>
    {#if data.ports.length}
      <div class="ports">
        {#each data.ports as p (p.id)}
          <div class="port" class:cabled={p.cabled}>
            {p.label}
            <Handle type="target" id={knobHandle(p.id)} position={Position.Left} class="in-mod t-{p.type}" style="left: -10px" />
          </div>
        {/each}
      </div>
    {/if}
    {#if data.knobs.length || data.hidden || graph.opened(id)}
      <div class="knobs">
        {#each data.knobs as k (k.id)}
          <div class="kw">
            <PatchKnob knob={k} onturn={(v) => turn(k, v)} ontap={() => graph.openKnob(id, k.channels ? `${k.id}.0` : k.id)} />
            <Handle type="target" id={knobHandle(k.id)} position={Position.Bottom} class="in-mod t-value" style="top: 17.5px; bottom: auto" />
            {#if k.next}<Handle type="target" id={knobHandle(nextPort(k.id))} position={Position.Left} class="in-mod t-trigger next" style="top: 6px; left: 2px" />{/if}
            {#if k.channels}
              <button class="poly nodrag" aria-label="{k.label}: each band" title="Each band" onclick={() => graph.toggle(pair(k))}>{k.channels.length}</button>
            {:else if polyOf(k) && graph.opened(pair(k)) && k.id.endsWith('.0')}
              <button class="poly nodrag" aria-label="{k.label}: as one" title="As one" onclick={() => graph.toggle(pair(k))}>−</button>
            {/if}
          </div>
        {/each}
        {#if data.hidden}
          <button class="more nodrag" onclick={() => graph.toggle(id)} aria-label="All knobs">+{data.hidden}</button>
        {:else if graph.opened(id)}
          <button class="more nodrag" onclick={() => graph.toggle(id)} aria-label="Fewer knobs">−</button>
        {/if}
      </div>
    {/if}
    {#if data.outs.length}
      <div class="outs">
        {#each data.outs as o (o.id)}
          <div class="jack">
            <i class="light t-{o.type}" style:--lit={lights?.[o.id] ?? 0}></i><span class="jl">{o.label}</span>
            <Handle type="source" id={o.handle} position={Position.Top} isConnectable={o.connectable || o.handle === MOD_OUT} class="out t-{o.type}" />
          </div>
        {/each}
      </div>
    {/if}
    {#if data.face && data.face.kind !== 'clock'}<PatchFace {id} face={data.face} />{/if}
    {#if data.in}<Handle type="target" id="in" position={SIDE[data.in]} isConnectable={!!data.v2} class="pic t-picture" style={at(data.in)} />{/if}
    {#if data.out}<Handle type="source" id="out" position={SIDE[data.out]} isConnectable={!!data.v2} class="pic" style={at(data.out)} />{/if}
    {#if data.feedback}<Handle type="source" id="fb" position={Position.Bottom} isConnectable={false} class="fb" />{/if}
    {#if data.feedbackIn}<Handle type="target" id="fbin" position={Position.Bottom} isConnectable={false} class="fb" />{/if}
    {#if data.modulator && !data.outs.length}
      <Handle type="source" id={MOD_OUT} position={Position.Top} class="out t-{data.gives?.type ?? 'value'}" />
    {/if}
  </div>
{:else}
  <div class="node mark" class:sel={selected}>
    <span class="t">{data.label}</span>
    <span class="sum only">{data.label}</span>
    <Handle type="target" id="in" position={SIDE[data.in]} isConnectable={false} class="pic" />
    {#if data.out}<Handle type="source" id="out" position={SIDE[data.out]} isConnectable={false} class="pic" />{/if}
  </div>
{/if}

<style>
  .box {
    width: 100%;
    height: 100%;
    border: 1px dashed color-mix(in srgb, var(--text) 16%, transparent);
    border-radius: 14px;
  }
  /* A device opened in place: its frame, a little firmer than a layer's. */
  .box.dev {
    border-style: solid;
    border-color: color-mix(in srgb, var(--text) 22%, transparent);
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .bl {
    position: absolute;
    left: 10px;
    top: -9px;
    padding: 0 6px;
    background: var(--patch-bg);
    color: var(--muted);
    font-size: max(12px, calc(11px / var(--z, 1)));
  }
  .node {
    position: relative;
    min-width: 140px;
    padding: 8px 10px 10px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--tile);
    color: var(--text);
    font-size: 13.5px;
  }
  .node.src {
    border-color: color-mix(in srgb, var(--text) 22%, var(--border));
  }
  .node.mod {
    min-width: 150px;
    background: color-mix(in srgb, var(--patch-mod) 12%, var(--surface));
    border-color: color-mix(in srgb, var(--patch-mod) 35%, var(--border));
  }
  .node.mark {
    min-width: 0;
    padding: 8px 12px;
  }
  /* A layer that is off: one faint row. */
  .node.off {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-width: 0;
    padding: 8px 12px;
    border-style: dashed;
    background: none;
    color: var(--muted);
  }
  .back,
  .node.add {
    color: var(--muted);
    font-size: 12.5px;
  }
  .back:hover,
  .node.add:hover,
  .plus:hover {
    color: var(--text);
  }
  .node.add {
    min-width: 0;
    padding: 6px 12px;
    border-style: dashed;
    background: none;
  }
  .plus {
    margin-left: 6px;
    padding: 0 4px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1;
  }
  .node.sel {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .t {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-weight: 600;
    white-space: nowrap;
  }
  button {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
  }
  .chev {
    color: var(--muted);
    font-weight: 400;
  }
  .ports {
    margin-top: 6px;
    display: grid;
    gap: 2px;
  }
  .port {
    position: relative;
    height: 18px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .port.cabled {
    color: var(--text);
  }
  .knobs {
    display: flex;
    flex-wrap: wrap;
    max-width: calc(6 * 44px);
    gap: 6px 0;
    margin-top: 8px;
    align-items: flex-start;
  }
  .kw {
    position: relative;
  }
  .poly {
    position: absolute;
    top: -3px;
    right: 3px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--surface);
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
  }
  .poly:hover {
    color: var(--text);
  }
  .more {
    width: 44px;
    height: 26px;
    border-radius: 13px;
    color: var(--muted);
    font-size: 12px;
  }
  .more:hover {
    color: var(--text);
  }
  .node :global(.svelte-flow__handle) {
    width: 9px;
    height: 9px;
    min-width: 0;
    min-height: 0;
    border: 0;
    border-radius: 50%;
  }
  .node :global(.svelte-flow__handle.pic) {
    background: var(--text);
  }
  .node :global(.svelte-flow__handle.out) {
    background: var(--patch-mod);
    cursor: crosshair;
  }
  .node :global(.svelte-flow__handle.in-mod) {
    width: 7px;
    height: 7px;
    background: color-mix(in srgb, var(--patch-mod) 35%, transparent);
  }
  .node :global(.svelte-flow__handle.in-mod.connectingto),
  .node :global(.svelte-flow__handle.in-mod:hover) {
    background: var(--patch-mod);
  }
  /* By type: a value's purple, a trigger's a diamond, the sound's a ring, light amber (PatchGraph's --wire-*). */
  .node :global(.svelte-flow__handle.t-trigger) {
    background: var(--wire-trigger);
    border-radius: 0;
    clip-path: polygon(50% 0, 100% 50%, 50% 100%, 0 50%);
    width: 10px;
    height: 10px;
  }
  .node :global(.svelte-flow__handle.in-mod.t-trigger) {
    background: color-mix(in srgb, var(--wire-trigger) 55%, transparent);
  }
  .node :global(.svelte-flow__handle.t-audio) {
    background: var(--patch-bg);
    box-shadow: inset 0 0 0 2px var(--wire-audio);
    width: 10px;
    height: 10px;
  }
  .node :global(.svelte-flow__handle.t-light) {
    background: var(--wire-light);
  }
  /* Points: a small square. */
  .node :global(.svelte-flow__handle.t-points) {
    background: var(--wire-points);
    border-radius: 2px;
  }
  .node :global(.svelte-flow__handle.in-mod.t-trigger.connectingto),
  .node :global(.svelte-flow__handle.in-mod.t-trigger:hover) {
    background: var(--wire-trigger);
  }
  .outs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 2px;
    max-width: calc(6 * 44px);
    margin-top: 10px;
  }
  .jack {
    position: relative;
    display: grid;
    justify-items: center;
    gap: 2px;
    min-width: 26px;
    max-width: 44px;
    padding-top: 6px;
    font-size: 11px;
    color: var(--muted);
  }
  .jl {
    max-width: 44px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .light {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--wire-value) 25%, transparent);
  }
  .light.t-trigger {
    background: color-mix(in srgb, var(--wire-trigger) calc(18% + 82% * var(--lit, 0)), transparent);
    box-shadow: 0 0 calc(6px * var(--lit, 0)) var(--wire-trigger);
  }
  /* Text that would be under 11 px on screen is not drawn (PatchGraph's .small: zoomed under 1). Far out (.far: so far
     that a title would be), a node is its name and how much of it is cabled, over it at 11 px or more. */
  :global(.small) .node .port,
  :global(.small) .node .jl,
  :global(.small) .node .poly,
  :global(.small) .node .more,
  :global(.small) .node :global(.knob span) {
    visibility: hidden;
  }
  .sum {
    display: none;
    position: absolute;
    inset: 0;
    z-index: 1;
    padding: 4px 8px;
    border-radius: inherit;
    background: inherit;
    color: var(--muted);
    font-size: calc(11px / var(--z, 1));
    line-height: 1.15;
    text-align: left;
    overflow: hidden;
  }
  .sum b {
    color: var(--text);
  }
  :global(.far) .node .sum {
    display: flex;
    align-items: center;
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sum.only {
    color: var(--text);
    font-weight: 600;
  }
  /* Far out the boxes' names give way to the chips; a row's or a mark's own text to its one line. */
  :global(.far) .bl,
  :global(.far) .node.add,
  :global(.far) .node.off .back {
    visibility: hidden;
  }
  /* A category's lane's header (v2): its name, its pin, its "+". */
  .lane {
    display: flex;
    gap: 8px;
    align-items: center;
    /* Its size fixed (the layout's), its name growing past it as the view zooms out, never under 11 px. */
    width: 220px;
    height: 26px;
    overflow: visible;
    white-space: nowrap;
    color: var(--muted);
    font-size: max(12.5px, calc(11px / var(--z, 1)));
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .pin {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 12px;
    color: var(--muted);
    opacity: 0.6;
  }
  .pin.on {
    color: var(--accent);
    opacity: 1;
  }
  .pin:hover {
    opacity: 1;
  }
  .title,
  .t {
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A finger's handles: 24 px and more to hit; a knob's under it, clear of the dial. */
  @media (pointer: coarse) {
    .node :global(.svelte-flow__handle)::after {
      content: '';
      position: absolute;
      inset: -9px;
      border-radius: 50%;
    }
    .kw :global(.svelte-flow__handle.in-mod) {
      top: auto !important;
      bottom: -6px !important;
    }
  }
  /* A step knob's next: a small diamond at its dial's upper left. */
  .node :global(.svelte-flow__handle.next) {
    width: 8px;
    height: 8px;
  }
  .node :global(.svelte-flow__handle.fb) {
    opacity: 0;
    pointer-events: none;
  }
</style>
