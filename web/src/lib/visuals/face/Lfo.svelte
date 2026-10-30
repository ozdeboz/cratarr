<script lang="ts">
  // The LFO's face (faces.ts): its wave from its knobs, a dot at its phase (worked out from its output and which way it
  // is going). Its memory kept here.
  import { lfoDot, lfoPath, lfoStep, newLfoMemo, type LfoKnobs, type LfoMemo, type LiveFrame } from '../faces';
  import { FACE_W, liveValue } from './face';

  let { live, path, knobs, width = FACE_W }: { live: LiveFrame | null; path: string; knobs: Record<string, number>; width?: number } = $props();

  const H = 26;
  const k = $derived({ shape: knobs.shape ?? 0, amount: knobs.amount ?? 1 } as LfoKnobs);
  let wave: LfoMemo = newLfoMemo();
  const dot = $derived.by(() => {
    const f = live;
    wave = lfoStep(wave, f ? liveValue(f.outputs, path) : null);
    return lfoDot(wave, k, width, H);
  });
</script>

<svg class="lfo" {width} height={H} viewBox="0 0 {width} {H}" aria-hidden="true">
  <path d={lfoPath(k.shape, width, H)} class="shape" />
  {#if live && k.amount > 0}<circle cx={dot.x} cy={dot.y} r="2.4" class="dot" />{/if}
</svg>

<style>
  .lfo {
    display: block;
    overflow: visible;
  }
  .shape {
    fill: none;
    stroke: color-mix(in srgb, var(--meter, var(--patch-mod)) 55%, transparent);
    stroke-width: 1.4;
    stroke-linejoin: round;
  }
  .dot {
    fill: var(--text);
  }
</style>
