<script lang="ts" module>
  /** A setting's value as written (the slider's, and Settings' row): whole for Bands, else a short decimal. */
  export const bandValueText = (value: number, whole = false) =>
    whole ? Math.round(value).toString() : value < 1 ? value.toFixed(2).replace(/0$/, '') : value < 10 ? value.toFixed(1).replace(/\.0$/, '') : Math.round(value).toString();
</script>

<script lang="ts">
  // One of the bands' settings as a slider (the owner, 2026-10-24: sliders, not steps): Speed and Window (lib/BandGlass),
  // in Settings › Display › Player visualisation and the player's own sheet (lib/BandSheet). Even in feel: each step is
  // the same share more, value = min × (max / min)^position; `linear` for one from 0 (Gap). `whole` keeps it a whole
  // number (Bands). Its ends named; its value after it when `unit` is given (or it is `whole`).
  import { panSafe } from './panSafe';
  let {
    label,
    range,
    value = $bindable(),
    ends,
    unit = null,
    linear = false,
    whole = false,
    wide = false,
  }: {
    label: string;
    range: { min: number; max: number };
    value: number;
    ends: [string, string];
    /** Shown after the slider with the value, as `3 s`; none without. */
    unit?: string | null;
    linear?: boolean;
    whole?: boolean;
    /** As wide as where it is (the player's sheet), the slider taking the room. */
    wide?: boolean;
  } = $props();

  const STEPS = 100;
  const span = $derived(linear ? range.max - range.min : Math.log(range.max / range.min));
  const at = $derived(Math.round(((linear ? value - range.min : Math.log(value / range.min)) / span) * STEPS));
  const shown = $derived(bandValueText(value, whole));
  function set(position: number) {
    const v = linear ? range.min + (position / STEPS) * span : range.min * Math.exp((position / STEPS) * span);
    value = whole ? Math.round(v) : v;
  }
</script>

<span class="slider" class:wide>
  <small>{ends[0]}</small>
  <input
    type="range" use:panSafe
    min="0"
    max={STEPS}
    step="1"
    value={at}
    aria-label={label}
    oninput={(e) => set(Number(e.currentTarget.value))}
  />
  <small>{ends[1]}</small>
  {#if unit || whole}<output>{shown}{unit ? ` ${unit}` : ''}</output>{/if}
</span>

<style>
  .slider {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .wide {
    display: flex;
    width: 100%;
  }

  .wide input {
    flex: 1;
    width: auto;
  }

  input {
    width: 11em;
    max-width: 100%;
    accent-color: var(--art-accent, var(--accent));
  }

  small {
    color: var(--muted);
  }

  output {
    min-width: 4.5ch;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
