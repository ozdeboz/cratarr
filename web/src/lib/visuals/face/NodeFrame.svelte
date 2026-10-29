<script lang="ts">
  // The one frame every module and device node has (the owner: "a standard panel for every module"): a header (its name
  // on its category's colour, its pin, a device's "+"), its jacks (inputs down the left, outputs down the right, each
  // labelled: face/Jack), its knobs in a grid (face/Knob, each with its input under its name and a step knob's
  // trigger and a level knob's gain beside it), then its face (face/Face: meters, scopes, lights, a thumbnail, its
  // shape). Far out it is one line over itself (its name and how much is cabled) on its far sockets. Tapping its name
  // opens its page (a device: into it); a long press (or right click), its menu.
  import { getContext } from 'svelte';
  import { Handle, Position, useUpdateNodeInternals } from '@xyflow/svelte';
  import { longpress } from '../../longpress';
  import Icon from '../../Icon.svelte';
  import Knob from './Knob.svelte';
  import Jack from './Jack.svelte';
  import Face from './Face.svelte';
  import { areaParts, partValue } from './face';
  import { clockLights } from '../faces';
  import { countLabel, GRAPH, gainPort, knobHandle, nextPort, polyTurned, type GraphActions, type KnobView, type ModuleData } from '../patchgraph';

  let { id, data, selected }: { id: string; data: ModuleData; selected?: boolean } = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const update = useUpdateNodeInternals();

  // A polyphonic knob turned as one: every channel as far, in one change.
  function turn(k: KnobView, v: number) {
    if (!k.channels) return graph.knob(id, k.id, v);
    graph.knobs(id, k.id, polyTurned(k, v));
  }
  // A polyphonic knob opened out to its channels, or one of them back to the one.
  const polyOf = (k: KnobView) => (k.channels ? k.id : /^(.+)\.\d+$/.exec(k.id)?.[1]);
  const pair = (k: KnobView) => `${id}.${polyOf(k)}`;
  // A knob's jack: under its label (as a module's panel has it), clear of the dial and the name.
  const knobJack = 'top: auto; bottom: -8px';

  // The far form's sockets: what comes in on the side the node takes in, what goes out on the other.

  // Its face: what it reads live (the graph's frame, by the module's path from the top), the lights on its outputs.
  const live = $derived(data.face ? graph.live() : null);
  const path = $derived(graph.livePath(id));
  const outIds = $derived(data.outs.map((o) => o.id));
  const lit = $derived(new Set((data.face?.parts ?? []).filter((p) => p.kind === 'light').map((p) => p.of ?? 'out')));
  const area = $derived(data.face ? areaParts(data.face.parts, outIds) : []);
  // The Clock's lights from the frame's beat where the engine gives none of its own.
  const beatLights = $derived(data.type === 'clock' ? (clockLights(live?.beat ?? null) as Record<string, number>) : null);
  function lightOf(port: string): number | null | undefined {
    if (!lit.has(port)) return undefined;
    const v = partValue(live?.outputs, path, { kind: 'light', of: port }, {});
    return v ?? beatLights?.[port] ?? null;
  }

  // The handles move when the knobs, ports or jacks shown change: measured again.
  const shape = $derived(data.knobs.map((k) => k.id).join() + '|' + data.ports.map((p) => p.id).join() + '|' + data.outs.length + '|' + (data.face?.lanes ?? 0) + '|' + area.length);
  $effect(() => {
    void shape;
    update(id);
  });
</script>

<div
  class="node cat-{data.category ?? 'none'}"
  style:--room={data.room ? `${data.room}px` : null}
  class:mod={data.modulator}
  class:sel={selected}
  ondblclick={() => graph.open(id)}
  role="presentation"
  use:longpress={() => graph.menu(id)}
>
  <span class="title">
    <button class="t nodrag" onclick={() => graph.open(id)}>{data.label}<span class="chev">›</span></button>
    {#if data.pinned}<span class="pinned" title="Pinned" aria-label="Pinned"><Icon name="pin" size={13} /></span>{/if}
    {#if data.device}<button class="plus nodrag" aria-label="Open {data.label} here" title="Open here" onclick={() => graph.toggle(id)}>+</button>{/if}
  </span>
  <!-- Far out (no knobs to turn), the whole node is its drag handle and its long press (the node's: select mode): its one
       line is no nodrag; a drag past the threshold moves it before the hold's 500 ms; its jacks stay jacks. -->
  <button class="sum" tabindex="-1" onclick={() => graph.open(id)}
    ><b>{data.label}</b>{#if countLabel(data.cables)}<span class="n">{' · '}{countLabel(data.cables)}</span>{/if}</button
  >
  {#if data.in || data.out || data.ports.length || data.outs.length}
    {@const pin = data.v2.pin}
    {@const pout = data.v2.pout}
    <div class="io">
      <div class="ins">
        {#if data.in && pin}
          <Jack dir="in" handle="in" picture type={data.pinType ?? 'picture'} label={data.v2.ports.in.find((x) => x.id === pin)?.label ?? 'In'} onhelp={() => graph.help(id, pin)} />
        {/if}
        {#each data.ports as p (p.id)}
          <Jack dir="in" handle={knobHandle(p.id)} type={p.type} label={p.label} cabled={p.cabled} onhelp={() => graph.help(id, p.id)} />
        {/each}
      </div>
      <div class="outs">
        {#if data.out && pout}
          <Jack dir="out" handle="out" picture type={data.poutType ?? 'picture'} label={data.v2.ports.out.find((x) => x.id === pout)?.label ?? 'Out'} onhelp={() => graph.help(id, pout)} />
        {/if}
        {#each data.outs as o (o.id)}
          <Jack dir="out" handle={o.handle} type={o.type} label={o.label} light={lightOf(o.id)} onhelp={() => graph.help(id, o.id)} />
        {/each}
      </div>
    </div>
  {/if}
  {#if data.knobs.length || data.hidden || graph.opened(id)}
    <div class="knobs">
      {#each data.knobs as k (k.id)}
        <div class="kw">
          <Knob knob={k} onturn={(v) => turn(k, v)} ontap={() => graph.openKnob(id, k.id)} onfine={(f) => graph.fine(f && { ...f, node: id, knob: k.id, label: k.label, min: k.min, max: k.max, kind: k.kind, options: k.options })} />
          <Handle type="target" id={knobHandle(k.id)} position={Position.Bottom} class="jack in-mod t-value" style={knobJack} />
          {#if k.next}<Handle type="target" id={knobHandle(nextPort(k.id))} position={Position.Left} class={['jack in-mod t-trigger next', !k.nextOn && 'quiet']} style="top: 6px; left: 2px" />{/if}
          {#if k.gain}<Handle type="target" id={knobHandle(gainPort(k.id))} position={Position.Right} class={['jack in-mod t-value next', !k.gainOn && 'quiet']} style="top: 6px; right: 2px" />{/if}
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
  {#if data.face && area.length}<Face parts={area} {live} {path} knobs={data.face.knobs} lanes={data.face.lanes} thumbs={graph.thumbs()} />{/if}
</div>

<style>
  .node {
    /* Opaque, so no cable shows through a node. */
    --node-bg: var(--tile);
    --cat: var(--border);
    position: relative;
    min-width: 140px;
    padding: 8px 10px 10px;
    border-radius: 12px;
    border: 1px solid var(--border);
    /* Its header's edge in its category's colour (rack.ts's five). */
    border-top: 3px solid var(--cat);
    background: linear-gradient(var(--node-bg), var(--node-bg)), var(--patch-bg);
    color: var(--text);
    font-size: 13.5px;
  }
  .node.cat-sources {
    --cat: var(--cat-sources);
  }
  .node.cat-effects {
    --cat: var(--cat-effects);
  }
  .node.cat-mixers {
    --cat: var(--cat-mixers);
  }
  .node.cat-modulators {
    --cat: var(--cat-modulators);
  }
  .node.cat-io {
    --cat: var(--cat-io);
  }
  .node.mod {
    min-width: 150px;
    --node-bg: color-mix(in srgb, var(--patch-mod) 12%, var(--surface));
    background: linear-gradient(var(--node-bg), var(--node-bg)), var(--patch-bg);
    border-color: color-mix(in srgb, var(--patch-mod) 35%, var(--border));
    border-top-color: var(--cat);
  }
  .node.sel {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 4px;
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
  .pinned {
    align-self: center;
    color: var(--accent-ink);
    line-height: 0;
  }
  .plus {
    margin-left: 6px;
    padding: 0 4px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1;
  }
  .plus:hover {
    color: var(--text);
  }
  /* Its jacks: inputs down the left, outputs down the right, each with its name. */
  .io {
    display: grid;
    grid-template-columns: auto auto;
    justify-content: space-between;
    column-gap: 16px;
    margin-top: 6px;
  }
  .ins,
  .outs {
    display: grid;
    gap: 2px;
    align-content: start;
  }
  .knobs {
    display: flex;
    flex-wrap: wrap;
    max-width: calc(6 * 44px);
    /* Room under each knob for its jack, clear of the row below. */
    gap: 16px 0;
    padding-bottom: 8px;
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
  /* Every socket a jack (after VCV Rack's, drawn our own way): a ring in its wire's colour round a dark hole, all one
     size (patchgraph.ts JACK), so a cable ends at its centre with its plug over it. A trigger's ring is a diamond. */
  .node :global(.jack) {
    --jack-ring: var(--muted);
    width: 10px;
    height: 10px;
    min-width: 0;
    min-height: 0;
    border: 0;
    border-radius: 50%;
    background: var(--jack-hole, #0b0c0f);
    box-shadow: inset 0 0 0 2px var(--jack-ring);
  }
  .node :global(.jack.t-picture) {
    --jack-ring: var(--text);
  }
  .node :global(.jack.t-value) {
    --jack-ring: var(--wire-value);
  }
  .node :global(.jack.t-audio) {
    --jack-ring: var(--wire-audio);
  }
  .node :global(.jack.t-light) {
    --jack-ring: var(--wire-light);
  }
  .node :global(.jack.t-points) {
    --jack-ring: var(--wire-points);
  }
  /* A flow's jack: its ring doubled (the wire's double line), so a field's socket is told from a value's. */
  .node :global(.jack.t-flow) {
    --jack-ring: var(--wire-flow);
    box-shadow:
      inset 0 0 0 1.5px var(--jack-ring),
      inset 0 0 0 2.5px var(--jack-hole, #0b0c0f),
      inset 0 0 0 3.5px var(--jack-ring);
  }
  .node :global(.jack.t-trigger) {
    background: none;
    box-shadow: none;
  }
  .node :global(.jack.t-trigger)::before {
    content: '';
    position: absolute;
    inset: 1.5px;
    transform: rotate(45deg);
    border: 2px solid var(--wire-trigger);
    background: var(--jack-hole, #0b0c0f);
    pointer-events: none;
  }
  .node :global(.jack.out) {
    cursor: crosshair;
  }
  .node :global(.jack:hover),
  .node :global(.jack.connectingto) {
    box-shadow: inset 0 0 0 3px var(--jack-ring);
  }
  /* A step knob's diamond and a level knob's gain dot with nothing in them: shown only on the node in focus (hovered or
     selected), or while a cable is being drawn onto it. */
  .node :global(.jack.quiet) {
    opacity: 0;
    transition: opacity 0.12s;
  }
  .node:hover :global(.jack.quiet),
  .node.sel :global(.jack.quiet),
  .node :global(.jack.quiet.connectingto) {
    opacity: 1;
  }
  /* Text that would be under 11 px on screen is not drawn (PatchGraph's .small: zoomed under 1). Far out (.far: so far
     that a title would be), a node is its name and how much of it is cabled, over it at 11 px or more. */
  :global(.small) .node .poly,
  :global(.small) .node .more,
  :global(.small) .node :global(.knob span) {
    visibility: hidden;
  }
  /* Over the node from its left, as wide as its name needs, up to the room beside it (--room: never over the next
     node); a name longer than that ends in an ellipsis. */
  .sum {
    display: none;
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: max-content;
    min-width: 100%;
    max-width: var(--room, 100%);
    z-index: 1;
    padding: 0 calc(6px / var(--z, 1));
    border-radius: inherit;
    background: inherit;
    box-shadow: 0 0 0 calc(1px / var(--z, 1)) var(--border);
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
    justify-content: center;
  }
  /* Short of room, the count gives way first, then the name; each ends in an ellipsis, never cut. */
  .sum > b,
  .sum > .n {
    min-width: 0;
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sum > b {
    flex: 0 1 auto;
  }
  .sum > .n {
    flex: 1 1 0px;
  }
  /* Far out every jack stays where it is, over the node's one line, so each cable still lands on its own port. */
  :global(.far) .node :global(.jack) {
    z-index: 2;
  }
  /* At 100% a node is as wide as its title needs; only far out (its one line over it) does a name end in "…". */
  :global(.far) .node .title,
  :global(.far) .node .t {
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A finger's handles: 24 px and more to hit; a knob's under it, clear of the dial. */
  @media (pointer: coarse) {
    .node :global(.jack)::after {
      content: '';
      position: absolute;
      inset: -8px;
      border-radius: 50%;
    }
  }
</style>
