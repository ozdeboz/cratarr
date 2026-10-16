<script lang="ts">
  // A crate on the Crates page (decided 2026-10-16: big cards, "lots of information to show"): its picture (its own,
  // else a mosaic of its albums' covers), its name, how many tracks and how long, its top genres, how many of its
  // tracks were never played and how many came this week, and along its foot how much of it was played (explored).
  // Its favourites setting shows as the small heart after the count, as on the panel's toggle. A tap views it.
  import type { Crate } from './api';
  import { crateArt, FAVOURITES, favouritesOf } from './crates.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { clockText } from './remote.svelte';

  let { crate: c, active, onclick }: { crate: Crate; active: boolean; onclick: () => void } = $props();

  const art = $derived(crateArt(c.id));
  const fav = $derived(FAVOURITES[favouritesOf(c.id)]);
  const st = $derived(c.stats);
  const tracks = $derived(st?.tracks ?? c.counts[favouritesOf(c.id)]);
  const line = $derived(
    [tracks === null ? null : plural(tracks, 'track'), st?.seconds ? clockText(st.seconds) : null].filter(Boolean).join(' • '),
  );
  /** The share played, 0 to 1; null where not known. */
  const explored = $derived(st && st.unplayed !== null && st.tracks ? (st.tracks - st.unplayed) / st.tracks : null);
</script>

<button class="card" class:active {onclick} aria-label="View {c.name}">
  <span class="art" class:mosaic={art.length === 4}>
    {#each art as src, i (i)}<img {src} alt="" loading="lazy" decoding="async" />{:else}<span class="noart"><Icon name="crates" size={40} /></span>{/each}
  </span>
  <span class="text">
    <span class="name">{c.name}</span>
    <span class="line">
      {line}
      <span class="heart" class:filled={fav.filled} title={fav.label} aria-label={fav.label}><Icon name={fav.icon} size={12} /></span>
    </span>
    {#if st?.genres.length}<span class="genres">{st.genres.join(' • ')}</span>{/if}
    {#if st?.unplayed || st?.new_week}
      <span class="badges">
        {#if st.unplayed}<span class="badge">{st.unplayed.toLocaleString()} unplayed</span>{/if}
        {#if st.new_week}<span class="badge new">+{st.new_week.toLocaleString()} new</span>{/if}
      </span>
    {/if}
  </span>
  {#if explored !== null}
    <span class="explored" role="img" aria-label="{Math.round(explored * 100)}% played"><span style:width="{explored * 100}%"></span></span>
  {/if}
</button>

<style>
  .card {
    position: relative;
    display: flex;
    align-items: stretch;
    gap: 14px;
    width: 100%;
    min-height: 120px;
    padding: 0 12px 0 0;
    overflow: hidden;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: transform 80ms;
  }

  .card:active {
    transform: scale(0.98);
  }

  /* Viewed in the panel: a border only, as a playlist's chip. */
  .active {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .art {
    flex: none;
    display: grid;
    width: 120px;
    aspect-ratio: 1;
    align-self: start;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .art.mosaic {
    grid-template-columns: 1fr 1fr;
  }

  .art img {
    width: 100%;
    height: 100%;
    min-width: 0;
    object-fit: cover;
  }

  .noart {
    display: grid;
    place-items: center;
    color: var(--muted);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding: 12px 0 14px;
  }

  .name {
    display: -webkit-box;
    overflow: hidden;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.25;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .line,
  .genres {
    overflow: hidden;
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.75;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .heart {
    display: flex;
    color: var(--accent);
  }

  .heart.filled :global(svg) {
    fill: currentColor;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }

  .badge {
    padding: 2px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    font-size: 12px;
    font-weight: 500;
  }

  .badge.new {
    background: var(--accent);
    color: var(--accent-text);
  }

  /* How much of it was played, along its foot. */
  .explored {
    position: absolute;
    inset: auto 0 0;
    height: 3px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .explored span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
</style>
