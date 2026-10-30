<script lang="ts">
  // A module's face area (face.ts), its parts down it: meters, scopes, numbers, lights not on an output, a thumbnail,
  // the ADSR's and the LFO's shapes. The graph's node draws it under its knobs; the panel can draw it as it is. `live`:
  // the frame (LiveFaces); `path`: the module's path (its devices, `/`); `knobs`: its knobs as set.
  import type { LiveFrame } from '../faces';
  import { FACE_W, partValue, type FacePart, type Thumbs } from './face';
  import Meter from './Meter.svelte';
  import Scope from './Scope.svelte';
  import Light from './Light.svelte';
  import NumberView from './Number.svelte';
  import Thumbnail from './Thumbnail.svelte';
  import Adsr from './Adsr.svelte';
  import Lfo from './Lfo.svelte';

  let {
    parts,
    live,
    path,
    knobs,
    lanes = 1,
    thumbs = null,
    width = FACE_W,
  }: { parts: FacePart[]; live: LiveFrame | null; path: string; knobs: Record<string, number>; lanes?: number; thumbs?: Thumbs | null; width?: number } = $props();

  const value = (p: FacePart, channel?: number) => partValue(live?.outputs, path, p, knobs, channel);
  const t = $derived(live?.time ?? 0);
</script>

<div class="face" style:--face-w="{width}px">
  {#each parts as p, i (i)}
    {#if p.kind === 'meter'}
      {#if p.channels}
        <Meter values={Array.from({ length: p.channels }, (_, c) => (live ? value(p, c) : null))} max={p.max} {width} />
      {:else}
        <Meter value={value(p)} max={p.max} peak={p.peak} reduction={p.reduction} label={p.label} threshold={p.threshold ? (knobs[p.threshold] ?? null) : null} {t} {width} />
      {/if}
    {:else if p.kind === 'scope'}
      <Scope value={live ? value(p) : null} {t} threshold={p.threshold ? (knobs[p.threshold] ?? null) : null} {width} />
    {:else if p.kind === 'light'}
      <span class="lightrow"><Light value={value(p)} />{#if p.label}<span>{p.label}</span>{/if}</span>
    {:else if p.kind === 'number'}
      <NumberView value={value(p)} label={p.label} />
    {:else if p.kind === 'thumb'}
      <Thumbnail module={path} {thumbs} />
    {:else if p.kind === 'adsr'}
      <Adsr {live} {path} {knobs} {lanes} {width} />
    {:else if p.kind === 'lfo'}
      <Lfo {live} {path} {knobs} {width} />
    {/if}
  {/each}
</div>

<style>
  .face {
    display: grid;
    gap: 6px;
    width: var(--face-w);
    margin-top: 8px;
  }
  .lightrow {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 12px;
    color: var(--muted);
    font-size: 11px;
  }
</style>
