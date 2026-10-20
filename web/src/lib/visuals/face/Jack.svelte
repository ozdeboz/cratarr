<script lang="ts">
  // A Jack (face.ts's blocks; the graph's): one labelled socket on a node's edge, an input down its left (its name
  // after it) or an output down its right (its name before it, a Light beside it where its face has one). Its socket a
  // Svelte Flow handle drawn as a jack (the node's `.jack` styles: a ring in its wire's colour, a trigger's a diamond).
  import { Handle, Position } from '@xyflow/svelte';
  import { longpress } from '../../longpress';
  import Light from './Light.svelte';
  import type { PortType } from '../ports';

  let {
    dir,
    handle,
    type,
    label,
    cabled = false,
    picture = false,
    light,
    onhelp,
  }: {
    dir: 'in' | 'out';
    handle: string;
    type: PortType;
    label: string;
    cabled?: boolean;
    /** The node's picture (or light) socket. */
    picture?: boolean;
    /** Its light's value, where its face lights it (null: dark now). */
    light?: number | null;
    onhelp?: () => void;
  } = $props();
</script>

<div class="port" class:out={dir === 'out'} class:cabled use:longpress={() => onhelp?.()}>
  {#if dir === 'out' && light !== undefined}<Light value={light} {type} />{/if}<span class="pl">{label}</span>
  <Handle
    type={dir === 'in' ? 'target' : 'source'}
    id={handle}
    position={dir === 'in' ? Position.Left : Position.Right}
    class="jack {picture ? 'pic' : dir === 'in' ? 'in-mod' : 'out'} t-{type}"
    style={dir === 'in' ? 'left: -10px' : 'right: -10px'}
  />
</div>

<style>
  .port {
    position: relative;
    height: 18px;
    font-size: 11.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .port.out {
    display: flex;
    gap: 5px;
    align-items: center;
    justify-content: flex-end;
  }
  .port.cabled {
    color: var(--text);
  }
  /* Under 11 px on screen (the graph's .small), its name is not drawn. */
  :global(.small) .pl {
    visibility: hidden;
  }
</style>
