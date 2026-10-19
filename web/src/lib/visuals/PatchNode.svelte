<script lang="ts">
  // A node of the patch graph (patchgraph.ts): a module (its knobs, its picture in and out), a modulator or a signal
  // (an output to cable from), the blend, the screen, or a box around a layer or the loop. Tapping its title, or
  // double-clicking it, opens its page.
  import { getContext } from 'svelte';
  import { Handle, Position, useUpdateNodeInternals } from '@xyflow/svelte';
  import PatchKnob from './PatchKnob.svelte';
  import { GRAPH, knobHandle, MOD_OUT, type GraphActions, type NodeData, type Side } from './patchgraph';

  let { id, data, selected }: { id: string; data: NodeData; selected?: boolean } = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const update = useUpdateNodeInternals();

  const SIDE: Record<Side, Position> = { left: Position.Left, right: Position.Right, top: Position.Top, bottom: Position.Bottom };
  // Picture handles sit on the title's line at the sides.
  const at = (s: Side) => (s === 'left' || s === 'right' ? 'top: 19px' : undefined);
  const port = (p: string) => p.charAt(0).toUpperCase() + p.slice(1);

  // The handles move when the knobs shown change: measured again.
  const shape = $derived(data.kind === 'module' ? data.knobs.map((k) => k.id).join() + '|' + data.ports.length : '');
  $effect(() => {
    void shape;
    update(id);
  });
</script>

{#if data.kind === 'box'}
  <div class="box" ondblclick={() => graph.open(id)} role="presentation"><span class="bl">{data.label}</span></div>
{:else if data.kind === 'signal'}
  <div class="node mod" class:sel={selected} ondblclick={() => graph.open(id)} role="presentation">
    <span class="t">{data.label}</span>
    <Handle type="source" id={MOD_OUT} position={Position.Top} class="out" style="left: auto; right: -5px; top: 50%" />
  </div>
{:else if data.kind === 'module'}
  <div class="node" class:mod={data.modulator} class:src={data.place === 'source'} class:sel={selected} ondblclick={() => graph.open(id)} role="presentation">
    <button class="t nodrag" onclick={() => graph.open(id)}>{data.label}<span class="chev">›</span></button>
    {#if data.ports.length}
      <div class="ports">
        {#each data.ports as p (p.id)}
          <div class="port" class:cabled={p.cabled}>
            {port(p.id)}
            <Handle type="target" id={knobHandle(p.id)} position={Position.Left} class="in-mod" style="left: -10px" />
          </div>
        {/each}
      </div>
    {/if}
    {#if data.knobs.length || data.hidden || graph.opened(id)}
      <div class="knobs">
        {#each data.knobs as k (k.id)}
          <div class="kw">
            <PatchKnob knob={k} onturn={(v) => graph.knob(id, k.id, v)} />
            <Handle type="target" id={knobHandle(k.id)} position={Position.Bottom} class="in-mod" style="top: 26px; bottom: auto" />
          </div>
        {/each}
        {#if data.hidden}
          <button class="more nodrag" onclick={() => graph.toggle(id)} aria-label="All knobs">+{data.hidden}</button>
        {:else if graph.opened(id)}
          <button class="more nodrag" onclick={() => graph.toggle(id)} aria-label="Fewer knobs">−</button>
        {/if}
      </div>
    {/if}
    {#if data.in}<Handle type="target" id="in" position={SIDE[data.in]} isConnectable={false} class="pic" style={at(data.in)} />{/if}
    {#if data.out}<Handle type="source" id="out" position={SIDE[data.out]} isConnectable={false} class="pic" style={at(data.out)} />{/if}
    {#if data.feedback}<Handle type="source" id="fb" position={Position.Bottom} isConnectable={false} class="fb" />{/if}
    {#if data.modulator}
      <Handle type="source" id={MOD_OUT} position={Position.Top} class="out" style="left: auto; right: -5px; top: 19px" />
    {/if}
  </div>
{:else}
  <div class="node mark" class:sel={selected}>
    <span class="t">{data.label}</span>
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
  .bl {
    position: absolute;
    left: 10px;
    top: -9px;
    padding: 0 6px;
    background: var(--patch-bg);
    color: var(--muted);
    font-size: 12px;
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
  .node :global(.svelte-flow__handle.fb) {
    opacity: 0;
    pointer-events: none;
  }
</style>
