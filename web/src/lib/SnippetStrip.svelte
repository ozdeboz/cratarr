<script lang="ts">
  // A track's snippet on its waveform (0.12, the owner's mockup A2): the whole track as the player draws it (its bars by
  // lib/waveStrip's trackBars, at the player's 3 px), the snippet's stretch lit, what of it has sounded in the accent
  // and the rest of it lighter, a playhead running through it while it sounds. Under a track menu's head and in the crate
  // search's open row; nothing where the track has no waveform yet.
  import { lengthOf, prepareSnippet, snippet, stretchOf, type SnippetTrack } from './snippetPlayer.svelte';
  import { waveformOf } from './waveforms.svelte';
  import { trackBars } from './waveStrip';

  let { track }: { track: SnippetTrack } = $props();

  let width = $state(0);
  const LOW = 6;
  const bins = $derived(waveformOf(track.source, track.id) ?? null);
  const n = $derived(Math.max(16, Math.min(512, Math.floor(width / 3))));
  const heights = $derived(bins && width ? trackBars(bins, n) : []);
  $effect(() => prepareSnippet(track));
  const mine = $derived(snippet.key === `${track.source}/${track.id}`);
  // The stretch that sounds once it sounds (its start on a beat), else as the waveform finds it.
  const stretch = $derived(!bins ? null : mine && snippet.to > snippet.from ? { from: snippet.from, to: snippet.to } : stretchOf(track));
  /** The stretch and the playhead as shares of the track (its length from the stretch's own reckoning). */
  const ends = $derived.by(() => {
    if (!stretch) return null;
    const total = lengthOf(track);
    return total ? { a: stretch.from / total, b: stretch.to / total, total } : null;
  });
  const head = $derived(ends && mine && snippet.sounding ? Math.min(ends.b, Math.max(ends.a, snippet.at / ends.total)) : null);
  const f = (x: number) => Math.round(x * 100) / 100;
  /** The bars whose middle is in `lo`..`hi` (shares of the track) as one path. */
  function path(lo: number, hi: number) {
    let s = '';
    for (let i = 0; i < heights.length; i++) {
      const mid = (i + 0.5) / heights.length;
      if (mid < lo || mid >= hi) continue;
      const h = LOW + heights[i] * (100 - LOW);
      s += `M${f(i + 0.15)} ${f((100 - h) / 2)}h0.7v${f(h)}h-0.7z`;
    }
    return s;
  }
</script>

{#if bins && ends}
  <div class="strip" bind:clientWidth={width} aria-hidden="true">
    {#if heights.length}
      <svg viewBox="0 0 {heights.length} 100" preserveAspectRatio="none">
        <path class="rest" d={path(0, ends.a) + path(ends.b, 1.01)} />
        <path class="ahead" d={path(head ?? ends.a, ends.b)} />
        {#if head !== null}<path class="heard" d={path(ends.a, head)} />{/if}
      </svg>
      {#if head !== null}<span class="head" style:left="{head * 100}%"></span>{/if}
    {/if}
  </div>
{/if}

<style>
  .strip {
    position: relative;
    height: 34px;
    margin: 2px 0 6px;
  }

  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .rest {
    fill: color-mix(in srgb, var(--text) 22%, transparent);
  }

  .ahead {
    fill: color-mix(in srgb, var(--art-accent, var(--accent)) 45%, transparent);
  }

  .heard {
    fill: var(--art-accent, var(--accent));
  }

  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1.5px;
    margin-left: -0.75px;
    background: var(--text);
  }
</style>
