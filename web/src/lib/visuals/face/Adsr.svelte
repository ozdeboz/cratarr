<script lang="ts">
  // The ADSR's face (faces.ts): its shape from its knobs, the stage it is in lit and a dot where its output is on that
  // shape; a lane a channel (each with its channel's knobs). Its memory (the stage each lane is in) kept here.
  import { adsrDot, adsrPath, adsrStagePath, adsrStep, laneKnobs, newAdsrMemo, type AdsrMemo, type LiveFrame } from '../faces';
  import { FACE_W, LANE_H, liveValue } from './face';

  let { live, path, knobs, lanes = 1, width = FACE_W }: { live: LiveFrame | null; path: string; knobs: Record<string, number>; lanes?: number; width?: number } = $props();

  const H = $derived(lanes > 1 ? lanes * LANE_H : 26);
  const laneH = $derived(lanes > 1 ? LANE_H : H);
  const lane = $derived(Array.from({ length: lanes }, (_, c) => laneKnobs(knobs, c)));
  let memo: AdsrMemo[] = [];
  const stages = $derived.by(() => {
    const f = live;
    if (memo.length !== lanes) memo = Array.from({ length: lanes }, newAdsrMemo);
    memo = memo.map((m, c) => adsrStep(m, lane[c], f ? liveValue(f.outputs, path, 'out', lanes > 1 ? c : undefined) : null));
    return memo;
  });
</script>

<svg class="adsr" {width} height={H} viewBox="0 0 {width} {H}" aria-hidden="true">
  {#each stages as m, c (c)}
    {@const y0 = c * laneH + 1.5}
    {@const h = laneH - 3}
    <path d={adsrPath(lane[c], width, h, y0)} class="shape" />
    {#if m.stage !== 'idle'}
      {@const dot = adsrDot(m, lane[c], width, h, y0)}
      <path d={adsrStagePath(lane[c], m.stage, width, h, y0)} class="lit" />
      {#if dot}<circle cx={dot.x} cy={dot.y} r={lanes > 1 ? 1.8 : 2.4} class="dot" />{/if}
    {/if}
  {/each}
</svg>

<style>
  .adsr {
    display: block;
    overflow: visible;
  }
  path {
    fill: none;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .shape {
    stroke: color-mix(in srgb, var(--meter, var(--patch-mod)) 40%, transparent);
    stroke-width: 1.2;
  }
  .lit {
    stroke: var(--meter, var(--patch-mod));
    stroke-width: 2;
  }
  .dot {
    fill: var(--text);
  }
</style>
