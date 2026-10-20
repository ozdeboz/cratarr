<script lang="ts">
  // A node of the patch graph (patchgraph.ts): a module or a device (face/NodeFrame: the one frame every node has), the
  // frame round a device opened in place, or a category's lane's header (its pin, its "+").
  import { getContext } from 'svelte';
  import Icon from '../Icon.svelte';
  import NodeFrame from './face/NodeFrame.svelte';
  import { GRAPH, type GraphActions, type NodeData } from './patchgraph';

  let { id, data, selected }: { id: string; data: NodeData; selected?: boolean } = $props();
  const graph = getContext<GraphActions>(GRAPH);
</script>

{#if data.kind === 'box'}
  <div class="box" class:dev={data.device} ondblclick={() => graph.open(id)} role="presentation">
    <span class="bl"
      >{data.label}{#if data.device}<button
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
{:else}
  <NodeFrame {id} {data} {selected} />
{/if}

<style>
  button {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
  }
  .box {
    width: 100%;
    height: 100%;
    border: 1px dashed color-mix(in srgb, var(--text) 16%, transparent);
    border-radius: 14px;
  }
  .box.dev {
    border-style: solid;
    border-color: color-mix(in srgb, var(--text) 22%, transparent);
    background: color-mix(in srgb, var(--text) 3%, transparent);
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
  .plus:hover {
    color: var(--text);
  }
  .plus {
    margin-left: 6px;
    padding: 0 4px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1;
  }
  :global(.far) .bl {
    visibility: hidden;
  }
  .lane > span {
    padding: 0 6px;
    border-radius: 6px;
    background: var(--patch-bg);
  }
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
</style>
