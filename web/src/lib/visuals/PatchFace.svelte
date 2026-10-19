<script lang="ts">
  // A control source's live face on its graph node (faces.ts; VCV Rack's lights, their behaviour only): an ADSR's shape
  // with the stage it is in lit and a dot where its output is, a lane a channel; an LFO's wave with a dot at its phase.
  // Fed by the graph's live frame (at most ~30 a second); a few SVG paths, nothing more.
  import { getContext } from 'svelte';
  import { adsrDot, adsrPath, adsrStagePath, adsrStep, lfoDot, lfoPath, lfoStep, newAdsrMemo, newLfoMemo, outputOf, type AdsrKnobs, type AdsrMemo, type LfoKnobs, type LfoMemo } from './faces';
  import { FACE_W, faceHeight, GRAPH, LANE_H, type GraphActions, type ModuleData } from './patchgraph';

  let { id, face }: { id: string; face: NonNullable<ModuleData['face']> } = $props();
  const graph = getContext<GraphActions>(GRAPH);
  const H = $derived(faceHeight(face));
  const k = $derived(face.knobs as unknown as AdsrKnobs & LfoKnobs);

  // Each face's memory from sample to sample (its stage, which way it was going): kept here, not drawn.
  let lanes: AdsrMemo[] = [];
  let wave: LfoMemo = newLfoMemo();
  const adsr = $derived.by(() => {
    if (face.kind !== 'adsr') return [];
    const f = graph.live();
    if (lanes.length !== face.lanes) lanes = Array.from({ length: face.lanes }, newAdsrMemo);
    lanes = lanes.map((m, c) => adsrStep(m, k, f ? outputOf(f.outputs, id, 'out', face.lanes > 1 ? c : undefined) : null));
    return lanes;
  });
  const lfo = $derived.by(() => {
    if (face.kind !== 'lfo') return null;
    const f = graph.live();
    wave = lfoStep(wave, f ? outputOf(f.outputs, id) : null);
    return lfoDot(wave, k, FACE_W, H);
  });
  const laneH = $derived(face.lanes > 1 ? LANE_H : H);
</script>

{#if H}
  <svg class="face" width={FACE_W} height={H} viewBox="0 0 {FACE_W} {H}" aria-hidden="true">
    {#if face.kind === 'adsr'}
      {#each adsr as m, c (c)}
        {@const y0 = c * laneH + 1.5}
        {@const h = laneH - 3}
        <path d={adsrPath(k, FACE_W, h, y0)} class="shape" />
        {#if m.stage !== 'idle'}
          {@const dot = adsrDot(m, k, FACE_W, h, y0)}
          <path d={adsrStagePath(k, m.stage, FACE_W, h, y0)} class="lit" />
          {#if dot}<circle cx={dot.x} cy={dot.y} r={face.lanes > 1 ? 1.8 : 2.4} class="dot" />{/if}
        {/if}
      {/each}
    {:else if face.kind === 'lfo'}
      <path d={lfoPath(k.shape, FACE_W, H)} class="shape" />
      {#if lfo && k.amount > 0}<circle cx={lfo.x} cy={lfo.y} r="2.4" class="dot" />{/if}
    {/if}
  </svg>
{/if}

<style>
  .face {
    display: block;
    margin-top: 8px;
    overflow: visible;
  }
  path {
    fill: none;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .shape {
    stroke: color-mix(in srgb, var(--patch-mod) 40%, transparent);
    stroke-width: 1.2;
  }
  .lit {
    stroke: var(--patch-mod);
    stroke-width: 2;
  }
  .dot {
    fill: var(--text);
  }
</style>
