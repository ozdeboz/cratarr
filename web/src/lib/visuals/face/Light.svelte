<script lang="ts">
  // A Light (face.ts): a trigger's or a gate's LED, as a module's panel has them: lit as far as its value (0..1), and
  // flashing bright on a pulse (a rising edge through ½), fading over a quarter second. Its colour its wire's.
  import { untrack } from 'svelte';
  import { pulsed } from './face';
  import type { PortType } from '../ports';

  let { value, type = 'trigger', size = 6, label }: { value: number | null; type?: PortType; size?: number; label?: string } = $props();

  let before: number | null = null;
  let flashes = $state(0);
  $effect(() => {
    const now = value;
    if (pulsed(before, now)) untrack(() => flashes++);
    before = now;
  });
  const lit = $derived(value === null ? 0 : Math.min(1, Math.max(0, value)));
</script>

<span class="light t-{type}" style:--lit={lit} style:--size="{size}px" role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : 'true'}>
  {#key flashes}{#if flashes}<i class="flash"></i>{/if}{/key}
</span>

<style>
  .light {
    position: relative;
    display: inline-block;
    flex: none;
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    --led: var(--wire-value, var(--accent));
    background: color-mix(in srgb, var(--led) calc(18% + 82% * var(--lit, 0)), transparent);
    box-shadow: 0 0 calc(6px * var(--lit, 0)) var(--led);
  }
  .light.t-trigger {
    --led: var(--wire-trigger, var(--accent));
  }
  .light.t-audio {
    --led: var(--wire-audio, var(--accent));
  }
  .flash {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: var(--led);
    box-shadow: 0 0 8px var(--led);
    animation: flash 0.25s ease-out forwards;
  }
  @keyframes flash {
    from {
      opacity: 1;
    }
    to {
      opacity: 0;
    }
  }
</style>
