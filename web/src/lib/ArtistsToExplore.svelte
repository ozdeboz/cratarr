<script lang="ts">
  // Home's Artists to explore (the owner, 2026-10-23, look C; C4 on a phone): artists of the libraries switched on in a
  // new random order, the first open: a card washed in its photo's colour (else its biggest album's), its name, its
  // description (TheAudioDB's, kept by the server's artist store), its biggest albums; the others as one row of chips,
  // a tap opening that one instead. The name opens the artist; the description their page on its About, unfolded; the
  // fanned albums their page on its Albums; an album (the wide grid) opens the album.
  import { onMount } from 'svelte';
  import { albumCoverUrl, artistPhotoUrl, getHomeArtists, type ExploredArtist } from './api';
  import { hiddenLibraries } from './listFilter.svelte';
  import { pageAsked } from './artistAbout.svelte';
  import { albumSubject, artistSubject, viewOver } from './panel.svelte';
  import { prefs } from './prefs.svelte';
  import SectionHead from './SectionHead.svelte';

  /** `onsettled`: once its artists are in (or refused), so Home can show it in its turn. */
  let { onsettled }: { onsettled?: () => void } = $props();

  let artists = $state<ExploredArtist[]>([]);
  let open = $state(0);
  onMount(() => {
    getHomeArtists(prefs.source || undefined, hiddenLibraries()).then(
      (d) => ((artists = d.artists), onsettled?.()),
      () => ((artists = []), onsettled?.()),
    );
  });
  const a = $derived(artists[open]);
  /** The artist's page, at its About (unfolded) or its Albums. */
  function openAt(name: string, section: 'about' | 'albums') {
    pageAsked.name = name;
    pageAsked.section = section;
    viewOver(artistSubject('cratarr', name));
  }
  // The carousel: which card is in front as it is swiped; a chip brings its card.
  let row = $state<HTMLElement>();
  function settle() {
    clearTimeout(resting);
    if (!row) return;
    if (chosen) return void (chosen = false);
    const mid = row.scrollLeft + row.clientWidth / 2;
    let best = 0;
    let gap = Infinity;
    for (const el of row.querySelectorAll<HTMLElement>('.slide')) {
      const d = Math.abs(el.offsetLeft + el.offsetWidth / 2 - mid);
      if (d < gap) [gap, best] = [d, Number(el.dataset.at)];
    }
    open = best;
  }
  // Committed once, when the row comes to rest (the owner, 2026-10-26: the chip's highlight went back and forth as the
  // cards passed); a chip's own scroll keeps its choice. Without scrollend, a moment after the last scroll.
  let resting: ReturnType<typeof setTimeout> | undefined;
  let chosen = false;
  function scrolled() {
    clearTimeout(resting);
    resting = setTimeout(settle, 160);
  }
  $effect(() => () => clearTimeout(resting));
  function goTo(i: number) {
    open = i;
    chosen = true;
    row?.querySelector<HTMLElement>(`.slide[data-at="${i}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
  }
  const picture = (x: ExploredArtist) => (x.photo ? artistPhotoUrl(x.id) : albumCoverUrl('cratarr', x.albums[0]));
</script>

{#if a}
  <section aria-label="Artists to explore">
    <SectionHead title="Artists to explore" />
    <div class="wrap">
      <!-- A carousel (the owner, 2026-10-26): every artist's card in one row, swiped through, the next one peeking in;
           the chips under it say which and take it there. -->
      <div class="cards bleed" bind:this={row} onscroll={scrolled} onscrollend={settle}>
        {#each artists as x, i (x.id)}
          <div class="slide" data-at={i}>
            <article class="card">
              <div class="wash" aria-hidden="true"><img src={picture(x)} alt="" /></div>
              <div class="top">
                <button class="who" onclick={() => viewOver(artistSubject('cratarr', x.name))}>
                  {#if x.photo}<img class="face" src={artistPhotoUrl(x.id)} alt="" />{/if}
                  <span><b>{x.name}</b><small>{x.albums.length === 1 ? '1 album' : `${x.albums.length} albums`}</small></span>
                </button>
                <!-- On a phone a fanned stack beside the name (C4); wider, a 2×2 grid beside the text (C). -->
                <button class="fan" aria-label={`Albums by ${x.name}`} onclick={() => openAt(x.name, 'albums')}>
                  {#each x.albums.slice(0, 3).reverse() as id (id)}<img src={albumCoverUrl('cratarr', id)} alt="" />{/each}
                </button>
              </div>
              {#if x.bio}
                <button
                  class="bio"
                  onclick={() => openAt(x.name, 'about')}>{x.bio}</button
                >
              {/if}
              <ul class="grid">
                {#each x.albums as id (id)}
                  <li><button onclick={() => viewOver(albumSubject('cratarr', id))} aria-label="Open album"><img src={albumCoverUrl('cratarr', id)} alt="" /></button></li>
                {/each}
              </ul>
              {#if x.bio}<small class="credit">TheAudioDB</small>{/if}
            </article>
          </div>
        {/each}
      </div>
      <div class="chips bleed">
        {#each artists as x, i (x.id)}
          <button class="chip" class:on={i === open} aria-current={i === open || undefined} onclick={() => goTo(i)}><img src={picture(x)} alt="" loading="lazy" />{x.name}</button>
        {/each}
      </div>
    </div>
  </section>
{/if}

<style>
  .wrap {
    --bleed: 16px;
    padding: 0 16px;
  }

  /* One card a screen, the next peeking in; each card its own container (its wide layout from its own width). */
  .cards {
    display: flex;
    gap: 12px;
    overflow-x: auto;
    scroll-snap-type: x mandatory;
    scrollbar-width: none;
    overscroll-behavior-x: contain;
  }

  .cards::-webkit-scrollbar {
    display: none;
  }

  .slide {
    flex: 0 0 min(88%, 640px);
    container-type: inline-size;
    scroll-snap-align: center;
  }

  .slide > .card {
    height: 100%;
    box-sizing: border-box;
  }

  .chip.on {
    border-color: var(--accent);
  }

  /* The card, washed in the picture's colour and darkened enough that its text always reads. */
  .card {
    position: relative;
    isolation: isolate;
    overflow: hidden;
    display: grid;
    gap: 12px;
    padding: 18px 16px 16px;
    border-radius: 20px;
    background: var(--surface);
  }

  .wash {
    position: absolute;
    inset: -30%;
    z-index: -2;
    filter: blur(48px) saturate(1.6);
    opacity: 0.75;
  }

  .wash img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .card::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(180deg, rgb(10 12 16 / 0.35), rgb(10 12 16 / 0.72));
  }

  .top {
    display: grid;
    grid-template-columns: 1fr 112px;
    gap: 12px;
    align-items: center;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: #fff;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .who span {
    display: grid;
    min-width: 0;
  }

  .who b {
    font-size: 1.6rem;
    font-weight: 750;
    line-height: 1.1;
    letter-spacing: -0.01em;
  }

  .who small {
    color: rgb(255 255 255 / 0.65);
    font-size: 13px;
  }

  .face {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    object-fit: cover;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.45);
  }

  .fan {
    position: relative;
    height: 104px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .fan img {
    position: absolute;
    top: 10px;
    width: 84px;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 8px;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.5);
  }

  .fan img:nth-child(1) {
    left: 0;
    transform: rotate(-9deg);
  }

  .fan img:nth-child(2) {
    left: 14px;
    transform: rotate(-3deg);
  }

  .fan img:nth-child(3) {
    left: 28px;
    transform: rotate(4deg);
  }

  .bio {
    display: -webkit-box;
    margin: 0;
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    color: rgb(255 255 255 / 0.86);
    font-size: 0.94rem;
    /* Always as tall as its lines, a short biography or a long one: changing artists moves nothing (the owner,
       2026-10-24). */
    line-height: 1.4;
    height: calc(3 * 1.4em);
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }

  /* The albums as a grid only where there is room (C); on a phone the fan stands in for them. */
  .grid {
    display: none;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .grid button {
    display: block;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .grid img {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 10px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
  }

  .credit {
    justify-self: end;
    color: rgb(255 255 255 / 0.45);
    font-size: 11px;
  }

  @container (min-width: 560px) {
    .card {
      grid-template-columns: 1fr 220px;
      grid-template-areas: 'top grid' 'bio grid' 'credit grid';
      align-content: start;
      padding: 22px;
    }

    .top {
      grid-area: top;
      grid-template-columns: 1fr;
    }

    .fan {
      display: none;
    }

    .bio {
      grid-area: bio;
      height: calc(5 * 1.4em);
      -webkit-line-clamp: 5;
      line-clamp: 5;
    }

    .credit {
      grid-area: credit;
      justify-self: start;
      align-self: end;
    }

    .grid {
      grid-area: grid;
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
      align-self: center;
    }

    .who b {
      font-size: 1.8rem;
    }
  }

  /* The others: one row that scrolls, never wraps. */
  .chips {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px 4px 4px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    white-space: nowrap;
    cursor: pointer;
  }

  .chip img {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    object-fit: cover;
  }
</style>
