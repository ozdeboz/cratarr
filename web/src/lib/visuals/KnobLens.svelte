<script lang="ts">
  // A knob's fine adjust (the owner, after the player's waveform lens, lib/WaveLens): lifted clear of the finger, its
  // name, its value as a clear number, a magnified slider with the knob's place on it, the speed it moves at ("×¼":
  // the further up from it, the finer) and its one line of help. Drawn over the graph at true size, wherever the graph
  // is zoomed; it takes no pointer while the knob is held (the knob keeps the gesture). Let go, it stays settled with the
  // knob's pin (`onpin`, 0057: knob pins live in its long-press slider) until a tap elsewhere. Its look is the waveform
  // lens's: a card on the surface, an accent edge, a stem down to the finger.
  import Icon from '../Icon.svelte';
  import { fineText, speedLabel, type Fine } from './patchgraph';

  let { fine, help = null, pinned = false, onpin }: { fine: Fine; help?: string | null; pinned?: boolean; onpin?: () => void } = $props();

  const W = 264;
  const LIFT = 92;
  const at = $derived(fine.max > fine.min ? Math.min(1, Math.max(0, (fine.value - fine.min) / (fine.max - fine.min))) : 0);
  const left = $derived(Math.min(Math.max(8, fine.at.x - W / 2), (typeof innerWidth === 'number' ? innerWidth : 1600) - W - 8));
  // Fine ticks: a step knob's steps (at most 32), else twenty-fifths of the range.
  const ticks = $derived(fine.kind === 'step' ? Math.min(32, Math.round(fine.max - fine.min)) : 25);
  // Above the finger where there is room, else under it.
  let h = $state(150);
  const above = $derived(fine.at.y - LIFT - h > 8);
  const top = $derived(above ? fine.at.y - LIFT - h : fine.at.y + LIFT);
</script>

<div class="lens" class:settled={!!onpin} style:left="{left}px" style:top="{top}px" role="status" aria-live="polite" bind:clientHeight={h}>
  <div class="head">
    <span class="name">{fine.label}</span>
    {#if onpin}<button class="pin" class:on={pinned} aria-pressed={pinned} aria-label={pinned ? `Unpin ${fine.label}` : `Pin ${fine.label}`} title={pinned ? 'Unpin' : 'Pin'} onclick={onpin}
        ><Icon name="pin" size={16} /></button
      >{/if}
    <span class="value">{fineText(fine, fine.value)}</span>
  </div>
  <svg class="track" viewBox="0 0 {W - 28} 22" width={W - 28} height="22" aria-hidden="true">
    {#each Array.from({ length: ticks + 1 }, (_, i) => i) as i (i)}
      <line x1={(i / ticks) * (W - 28)} x2={(i / ticks) * (W - 28)} y1={i % 5 === 0 ? 4 : 8} y2="14" class="tick" />
    {/each}
    <rect x="0" y="17" width={W - 28} height="3" rx="1.5" class="well" />
    <rect x="0" y="17" width={at * (W - 28)} height="3" rx="1.5" class="fill" />
    <circle cx={at * (W - 28)} cy="18.5" r="5" class="dot" />
  </svg>
  <div class="foot">
    <span>{fineText(fine, fine.min)}</span>
    <span class="speed">{speedLabel(fine.speed)}</span>
    <span>{fineText(fine, fine.max)}</span>
  </div>
  {#if help}<p class="help">{help}</p>{/if}
</div>
<span class="stem" style:left="{fine.at.x - 1}px" style:top="{above ? fine.at.y - LIFT : fine.at.y + 14}px" style:height="{LIFT - 14}px" aria-hidden="true"></span>

<style>
  .lens,
  .stem {
    position: fixed;
    z-index: 30;
    pointer-events: none;
  }
  .lens {
    width: 264px;
    box-sizing: border-box;
    padding: 10px 14px 10px;
    border: 2px solid color-mix(in srgb, var(--accent) 70%, white);
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.55);
    color: var(--text);
  }
  /* Settled (let go): it takes taps, for its pin. */
  .lens.settled {
    pointer-events: auto;
  }
  .pin {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    margin-left: auto;
    border: 0;
    border-radius: 14px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .pin.on {
    color: var(--accent-ink);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
  }
  .name {
    color: var(--muted);
    font-size: 13px;
  }
  .value {
    font-size: 22px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .track {
    display: block;
    margin: 6px 0 2px;
    overflow: visible;
  }
  .tick {
    stroke: color-mix(in srgb, var(--text) 30%, transparent);
    stroke-width: 1;
  }
  .well {
    fill: color-mix(in srgb, var(--text) 16%, transparent);
  }
  .fill {
    fill: var(--accent);
  }
  .dot {
    fill: #fff;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    color: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .speed {
    color: var(--text);
    font-weight: 600;
  }
  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.35;
  }
  .stem {
    width: 2px;
    border-radius: 1px;
    background: color-mix(in srgb, var(--accent) 70%, white);
  }
</style>
