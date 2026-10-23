<script lang="ts" module>
  /** A density as a gradient (the owner, 2026-10-23: the histogram in the bar): each step's share of the fullest one
   *  as the colour's strength; an empty step is the bare track, so gaps show. `inside` (0 to 1 of the scale): steps
   *  outside it greyed (a slider's unchosen part). */
  export function densityGradient(bins: readonly number[], inside: [number, number] | null = null): string {
    const top = Math.max(1, ...bins);
    const stops = bins.map((c, i) => {
      const at = (i + 0.5) / bins.length;
      const a = c ? 0.1 + 0.9 * (c / top) : 0;
      const on = !inside || (at >= inside[0] && at <= inside[1]);
      const colour = on ? `color-mix(in srgb, var(--accent) ${Math.round(a * 100)}%, transparent)` : `color-mix(in srgb, var(--text) ${Math.round(a * 35)}%, transparent)`;
      return `${colour} ${(at * 100).toFixed(1)}%`;
    });
    return `linear-gradient(to right, ${stops.join(', ')})`;
  }
</script>

<script lang="ts">
  // One number of a list's profile (lib/ListProfile): its density over the scale as the bar's colour, its middle half
  // outlined and its middle a mark; the scale's ends under it. No values beside it: they read from the bar.
  let { bins, lo, hi, q1, mid, q3, ends }: { bins: readonly number[]; lo: number; hi: number; q1: number; mid: number; q3: number; ends: [string, string] } = $props();

  const at = (x: number) => Math.min(1, Math.max(0, (x - lo) / (hi - lo || 1))) * 100;
</script>

<div class="bar">
  <div class="track" style:background-image={densityGradient(bins)}>
    <div class="half" style:left="{at(q1)}%" style:width="{Math.max(1, at(q3) - at(q1))}%"></div>
    <div class="mid" style:left="{at(mid)}%"></div>
  </div>
  <div class="ends"><span>{ends[0]}</span><span>{ends[1]}</span></div>
</div>

<style>
  .bar {
    min-width: 0;
    padding-top: 6px;
  }

  .track {
    position: relative;
    height: 12px;
    border-radius: 6px;
    background-color: color-mix(in srgb, var(--text) 7%, transparent);
  }

  /* The middle half: an outline over the density, not a fill, so the density shows through. */
  .half {
    position: absolute;
    top: -3px;
    bottom: -3px;
    margin: 0 -2px;
    border: 1.5px solid color-mix(in srgb, var(--text) 55%, transparent);
    border-radius: 7px;
  }

  .mid {
    position: absolute;
    top: -5px;
    width: 3px;
    height: 22px;
    margin-left: -1.5px;
    border-radius: 2px;
    background: var(--text);
  }

  .ends {
    display: flex;
    justify-content: space-between;
    margin-top: 3px;
    color: var(--muted);
    font-size: 11.5px;
  }
</style>
