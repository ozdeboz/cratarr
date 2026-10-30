<script lang="ts">
  // A crate on the Crates page (decided 2026-10-16: big cards, "lots of information to show"): its picture (its own,
  // else a mosaic of its albums' covers), its name, how many tracks and how long, its top genres, how many of its
  // tracks were never played and how many came this week, and along its foot how much of it was played (explored).
  // Its favourites setting shows as the small heart after the count, as on the panel's toggle; its favourite albums
  // setting as the small disc beside it, as on its toggle (decision 0024, amended 2026-10-19). A tap views it, its
  // picture too, and it dims with the ring while it opens, when that takes long; Shuffle in its picture's upper right
  // corner alone plays it, shuffled (the owner, 2026-10-27: as every cover, lib/CoverPlay and lib/CoverWait; 2026-10-19:
  // "a play button on the crates, which automatically plays it with shuffle"). A long-press or a right-click opens its
  // menu (decision 0058).
  import type { Crate } from './api';
  import Cover, { mosaicOf } from './Cover.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import CoverWait from './CoverWait.svelte';
  import { CRATE, crateArt, FAVOURITE_ALBUMS, FAVOURITES, favouriteAlbumsOf, favouritesOf, shuffleCrate } from './crates.svelte';
  import { toast } from './toast.svelte';
  import { longpress } from './longpress';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { clockText } from './remote.svelte';

  // `why`: found by the search through its tracks (the server's deep search, decision 0072): which, and how many, in its
  // genres' place.
  let { crate: c, active, onclick, onmenu, why }: { crate: Crate; active: boolean; onclick: () => void; onmenu?: () => void; why?: string } = $props();

  const art = $derived(mosaicOf(crateArt(c.id)));
  const fav = $derived(FAVOURITES[favouritesOf(c.id)]);
  const albums = $derived(FAVOURITE_ALBUMS[favouriteAlbumsOf(c.id)]);
  const st = $derived(c.stats);
  const tracks = $derived(st?.tracks ?? c.counts[favouritesOf(c.id)]);
  const line = $derived(
    [tracks === null ? null : plural(tracks, 'track'), st?.seconds ? clockText(st.seconds) : null].filter(Boolean).join(' • '),
  );
  let starting = $state(false);
  async function shuffle() {
    navigator.vibrate?.(8);
    starting = true;
    try {
      await shuffleCrate(c.id, c.name);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }
  /** The share played, 0 to 1; null where not known. */
  const explored = $derived(st && st.unplayed !== null && st.tracks ? (st.tracks - st.unplayed) / st.tracks : null);
</script>

<div class="card" class:active>
  <button class="view" {onclick} aria-label="View {c.name}" use:longpress={() => onmenu?.()}>
  <span class="art" class:mosaic={art.length === 4}>
    {#each art as src, i (i)}<Cover {src} />{:else}<Cover src={null} />{/each}
    <CoverWait subject={CRATE + c.id} />
  </span>
  <span class="text">
    <span class="name">{c.name}</span>
    <span class="line">
      {line}
      <span class="heart" class:filled={fav.filled} title={fav.label} aria-label={fav.label}><Icon name={fav.icon} size={12} /></span>
      <span class="heart" class:filled={albums.filled} title={albums.label} aria-label={albums.label}><Icon name={albums.icon} size={12} /></span>
    </span>
    {#if why}<span class="genres why">{why}</span>{:else if st?.genres.length}<span class="genres">{st.genres.join(' • ')}</span>{/if}
    {#if st?.unplayed || st?.new_week}
      <span class="badges">
        {#if st.unplayed}<span class="badge">{st.unplayed.toLocaleString()} unplayed</span>{/if}
        {#if st.new_week}<span class="badge new">+{st.new_week.toLocaleString()} new</span>{/if}
      </span>
    {/if}
  </span>
  </button>
  <span class="shuffle-at"><CoverPlay shuffle label={c.name} busy={starting} onplay={shuffle} /></span>
  {#if explored !== null}
    <span class="explored" role="img" aria-label="{Math.round(explored * 100)}% played"><span style:width="{explored * 100}%"></span></span>
  {/if}
</div>

<style>
  .card {
    position: relative;
    display: flex;
    align-items: stretch;
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

  .card:has(.view:active) {
    transform: scale(0.98);
  }

  /* The rest of the card views the crate. */
  .view {
    display: flex;
    flex: 1;
    gap: 14px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  /* Shuffle over its picture's upper right corner (a button cannot hold another): a box as large as the picture,
     letting the taps through to it but on Shuffle. */
  .shuffle-at {
    position: absolute;
    top: 0;
    left: 0;
    width: 120px;
    aspect-ratio: 1;
    pointer-events: none;
  }

  .shuffle-at :global(.cover-play) {
    pointer-events: auto;
  }

  /* Viewed in the panel: a border only, as a playlist's chip; on the card's edge over everything in it, the covers and
     the played meter too (the owner, 2026-10-24: it ran under them). */
  .active::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 3;
    border-radius: inherit;
    box-shadow: inset 0 0 0 2px var(--accent);
    pointer-events: none;
  }

  .art {
    position: relative;
    flex: none;
    display: grid;
    width: 120px;
    aspect-ratio: 1;
    /* Square whatever the image's shape: without it a portrait cover stretched its box, and its card (the owner,
       2026-10-24). */
    overflow: hidden;
    align-self: start;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .art.mosaic {
    grid-template: 1fr 1fr / 1fr 1fr;
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

  .genres.why {
    color: var(--accent-ink);
    opacity: 1;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .heart {
    display: flex;
    color: var(--accent-ink);
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

  /* How much of it was played, along its foot: in the red, not the green the card's ring is when it is open (the owner,
     2026-10-24: the two clashed). */
  .explored {
    position: absolute;
    inset: auto 0 0;
    height: 3px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .explored span {
    display: block;
    height: 100%;
    background: var(--danger);
  }

  /* Open, the ring covers its foot's 2 px: the meter grows by as much, so as much of it shows (the owner, 2026-10-24);
     it runs under the ring at both sides, so no gap there. */
  .active .explored {
    height: 5px;
  }
</style>
