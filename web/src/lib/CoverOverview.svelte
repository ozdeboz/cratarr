<script lang="ts">
  // The cover's overview (the owner, 2026-10-19): a circle drawn on the full player's cover, or the info button at the
  // start of its row, blurs the cover and shows over it every fact known about the track (lib/trackFacts), in full and
  // in the owner's order: the genres as chips on top, energy and danceability as bars. Only what is known shows.
  import type { TrackInfo } from './api';
  import { detailsOf, factsOf, FACTS } from './trackFacts.svelte';

  let { track }: { track: { source: string; id: string; info?: TrackInfo | null } } = $props();

  const facts = $derived(factsOf(detailsOf(track.source, track.id), track.info ?? null));
  const genres = $derived(facts.genre?.full.split(' · ') ?? []);
  const shown = $derived(FACTS.filter((f) => f.id !== 'genre' && facts[f.id]).map((f) => ({ ...f, fact: facts[f.id]! })));
</script>

<span class="overview" aria-live="polite">
  {#if genres.length}<span class="genres">{#each genres as g (g)}<span>{g}</span>{/each}</span>{/if}
  {#each shown as f (f.id)}
    <span class="line">
      <span class="label">{f.label}</span>
      {#if f.fact.meter !== undefined}
        <span class="meter" role="img" aria-label={f.fact.full}><span style:width="{Math.round(f.fact.meter * 100)}%"></span></span>
      {:else}
        <span class="value">{f.fact.full}</span>
      {/if}
    </span>
  {/each}
</span>

<style>
  /* Over the blurred cover, its own text in white on the cover's darkened picture. */
  .overview {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
    padding: 8% 9%;
    border-radius: 8px;
    background: rgb(0 0 0 / 0.42);
    color: #fff;
    text-align: left;
    font-size: clamp(12px, 3.2cqi, 16px);
    overflow: hidden;
    pointer-events: none;
    animation: in 220ms ease-out;
  }

  @keyframes in {
    from {
      opacity: 0;
    }
  }

  .genres {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 4px;
  }

  .genres span {
    padding: 2px 10px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.18);
    font-weight: 600;
  }

  .line {
    display: grid;
    grid-template-columns: 7.5em 1fr;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .label {
    color: rgb(255 255 255 / 0.65);
  }

  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .meter {
    height: 6px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.2);
    overflow: hidden;
  }

  .meter span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--art-accent, var(--accent));
  }
</style>
