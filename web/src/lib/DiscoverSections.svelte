<script lang="ts">
  // Discover's sections (decision 0069, amended 2026-10-25: "a more dynamic page", each section the form its content
  // suits, inspired by Home's variety, not a copy of it): the pick of the day as a spread; the discography gaps of the
  // artists most played, as a shelf; the masonry around the artists played; the new artists' pictures as a flow;
  // the fresh releases as a row; the Weekly Exploration as the list it is. What
  // plays on Bandcamp comes first everywhere (lib/discoverSections). A tap on a suggestion opens it (`onopen`), a long-press its
  // menu (`onmenu`), as Home's albums; Play hears it on Bandcamp in Cratarr's own player (`onplay`).
  import { untrack } from 'svelte';
  import { discoverArtistPhotoUrl, getDiscoverArtist, type DiscoverAlbum } from './api';
  import Cover from './Cover.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import CoverFlow from './CoverFlow.svelte';
  import { goToArtist } from './albums.svelte';
  import { DISCOVER } from './discoverArtist';
  import type { Sections } from './discoverSections';
  import type { Tile } from './masonry';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SectionHead from './SectionHead.svelte';

  let {
    sec,
    facts,
    onopen,
    onmenu,
    owned,
    onplay,
    previewing = null,
  }: {
    sec: Sections<DiscoverAlbum>;
    /** A suggestion's facts in one line (its type, tracks, length, listeners). */
    facts: (a: DiscoverAlbum) => string;
    onopen: (a: DiscoverAlbum) => void;
    /** Its menu: a long-press, or the pick's More. */
    onmenu: (a: DiscoverAlbum) => void;
    /** How many albums the library has of an artist (the "Because you play" tiles). */
    owned: (name: string) => number;
    onplay: (a: DiscoverAlbum) => void;
    previewing?: string | null;
  } = $props();

  // New to you's front artist (the flow's), and their biography (TheAudioDB's, as the sync kept it; the owner,
  // 2026-10-25: an artist without a picture is never featured).
  let front = $state(0);
  const frontArtist = $derived(sec.newArtists[Math.min(front, Math.max(0, sec.newArtists.length - 1))]?.artist ?? null);
  let bio = $state<{ name: string; text: string | null; credit?: import('./api').PhotoCredit | null } | null>(null);
  $effect(() => {
    const name = frontArtist;
    if (!name || bio?.name === name) return;
    getDiscoverArtist(name).then(
      (x) => (bio = { name, text: x.bio, credit: x.photo_credit }),
      () => (bio = { name, text: null }),
    );
  });

  // The gaps whose covers came (Cover's onfail: none to show, so not shown).
  let coverless = $state<string[]>([]);
  const gaps = $derived(sec.gaps.filter((a) => a.cover && !coverless.includes(a.key)));

  // The masonry (lib/masonry, the owner 2026-10-25, 0.9.2): each tile in its grid's place; the artist's photo where the
  // artist store has one, else the words alone.
  const place = (t: Tile) => `grid-column: ${t.col + 1} / span ${t.w}; grid-row: ${t.row + 1} / span ${t.h};`;
  // The artist's photo: asked of the artist store first (it reads TheAudioDB where it has none yet), shown once it has.
  let photos = $state<Record<string, boolean>>({});
  const albumsOf = (n: number) => `${n} ${n === 1 ? 'album' : 'albums'} in your library`;
  $effect(() => {
    for (const name of sec.around.flatMap((g) => [g.seeds[0], ...(g.partner ? [g.partner] : [])])) {
      if (name in untrack(() => photos)) continue;
      photos[name] = false;
      getDiscoverArtist(name).then((a) => (photos[name] = a.photo), () => {});
    }
  });
</script>

<!-- The pick of the day: one suggestion large, why it is here as a quote. -->
{#if sec.pick}
  {@const a = sec.pick}
  <section class="pick" aria-label="Pick of the day">
    <button class="pick-cover" aria-label={`${a.album} by ${a.artist}`} onclick={() => onopen(a)} use:longpress={() => onmenu(a)}><Cover src={a.cover} /></button>
    <div class="pick-text">
      <span class="kicker">Pick of the day</span>
      <span class="pick-title">{a.album}</span>
      <span class="pick-by">{a.artist}{a.year ? ` · ${a.year}` : ''}</span>
      {#if facts(a)}<span class="pick-facts">{facts(a)}</span>{/if}
      {#if a.genres?.length}<span class="tags">{#each a.genres as g (g)}<span class="tag">{g}</span>{/each}</span>{/if}
      <blockquote>{a.why.join('. ')}</blockquote>
      <div class="pick-acts">
        <button class="play" disabled={previewing === a.key} aria-busy={previewing === a.key} onclick={() => onplay(a)}><Icon name="play" size={18} />Play on Bandcamp</button>
        <button class="more" aria-label="More" title="More" onclick={() => onmenu(a)}><Icon name="moreVert" size={20} /></button>
      </div>
    </div>
  </section>
{/if}

<!-- Discography gaps (the owner, 2026-10-25): one shelf, as Home's New for you, of the albums missing from the artists
     most played; a card whose cover does not come leaves (no coverless albums, as on Home). -->
{#if gaps.length}
  <section aria-label="Discography gaps">
    <SectionHead title="Discography gaps" />
    <div class="row">
      {#each gaps as a (a.key)}
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} onfail={() => (coverless = [...coverless, a.key])} /></span>
            <b>{a.album}</b>
            <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
          </button>
          {#if a.bandcamp}<CoverPlay label={a.album} busy={previewing === a.key} onplay={() => onplay(a)} />{/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<!-- Around the artists you play: the seeds in the middle, their suggestions in a masonry around them. -->
{#if sec.around.length}
  <section aria-label="Around artists you play">
    <SectionHead title={sec.around.length === 1 ? `Around ${[sec.around[0].seeds[0], sec.around[0].partner].filter(Boolean).join(' and ')}` : 'Around artists you play'} />
    <div class="arounds">
      {#each sec.around as g (g.seeds.join('|'))}
        {@const grid = g.grid}
        <div class="masonry" style={`--cols: ${grid.cols}; --rows: ${grid.rows};`}>
          <div class="seeds" class:photo={photos[g.seeds[0]]} style={place(grid.seed)}>
            {#if photos[g.seeds[0]]}<img src={discoverArtistPhotoUrl(g.seeds[0])} alt="" onerror={() => (photos[g.seeds[0]] = false)} />{/if}
            <span class="seed-text"><small>Because you play</small><b>{g.seeds[0]}</b>{#if owned(g.seeds[0])}<span class="owned">{albumsOf(owned(g.seeds[0]))}</span>{/if}</span>
          </div>
          {#if grid.partner && g.partner}
            <!-- The library's artist closest to the first, one place (the owner, 2026-10-25). -->
            <div class="seeds partner" class:photo={photos[g.partner]} style={place(grid.partner)}>
              {#if photos[g.partner]}<img src={discoverArtistPhotoUrl(g.partner)} alt="" onerror={() => g.partner && (photos[g.partner] = false)} />{/if}
              <span class="seed-text"><small>And</small><b>{g.partner}</b>{#if owned(g.partner)}<span class="owned">{albumsOf(owned(g.partner))}</span>{/if}</span>
            </div>
          {/if}
          {#each g.items as a (a.key)}
            {@const t = grid.tiles.find((x) => x.key === a.key)}
            {#if t}
              <div class="tile-box" style={place(t)}>
                <button class="tile" aria-label={`${a.album} by ${a.artist}`} onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
                  <Cover src={a.cover} />
                  <span class="tile-text"><b>{a.album}</b><small>{a.artist}</small></span>
                </button>
                {#if a.bandcamp}<CoverPlay label={a.album} busy={previewing === a.key} onplay={() => onplay(a)} />{/if}
              </div>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
  </section>
{/if}

<!-- New to you (the owner, 2026-10-25): the new artists' pictures as a flow (Home's, lib/CoverFlow), the front one's
     name and how many albums they have released under it; below, their biography with their album as a card to its right. -->
{#if sec.newArtists.length}
  {@const n = sec.newArtists[Math.min(front, sec.newArtists.length - 1)]}
  {@const a = n.items[0]}
  <section aria-label="New to you">
    <SectionHead title="New to you" />
    <div class="artist-flow">
      <CoverFlow
        covers={sec.newArtists.map((x) => discoverArtistPhotoUrl(x.artist))}
        top={0}
        foot={40}
        onfront={(i) => (front = i)}
        onopen={(i) => onopen(sec.newArtists[i].items[0])}
        onmenu={(i) => onmenu(sec.newArtists[i].items[0])}
      />
    </div>
    <div class="artist-caption">
      <button class="title" onclick={() => goToArtist(DISCOVER, n.artist)}>{n.artist}</button>
      {#if a.artist_albums}<span class="by">{a.artist_albums} {a.artist_albums === 1 ? 'album' : 'albums'}</span>{/if}
      {#if bio?.name === n.artist && bio.credit}<a class="photo-credit" href={bio.credit.url} target="_blank" rel="noopener noreferrer">Photo: {bio.credit.author} · {bio.credit.licence}</a>{/if}
    </div>
    <!-- Their biography, their album to its right with why it is here under it (the owner, 2026-10-25). -->
    <div class="artist-about">
      {#if bio?.name === n.artist && bio.text}<p>{bio.text}</p>{/if}
      <div class="album-col">
        {#if a.why.length}<small class="why">{a.why.join('. ')}</small>{/if}
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} /></span>
            <b>{a.album}</b>
            <small>{a.year ?? ''}</small>
          </button>
          {#if a.bandcamp}<CoverPlay label={a.album} busy={previewing === a.key} onplay={() => onplay(a)} />{/if}
        </div>
      </div>
    </div>
  </section>
{/if}

<!-- New from artists you have: the fresh releases, newest first. -->
{#if sec.fresh.length}
  <section aria-label="New from artists you have">
    <SectionHead title="New from artists you have" />
    <div class="row">
      {#each sec.fresh as a (a.key)}
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} /></span>
            <b>{a.album}</b>
            <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
          </button>
          {#if a.bandcamp}<CoverPlay label={a.album} busy={previewing === a.key} onplay={() => onplay(a)} />{/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<!-- Your Weekly Exploration: the playlist it is, as a list. -->
{#if sec.weekly.length}
  <section aria-label="Your Weekly Exploration">
    <SectionHead title="Your Weekly Exploration" />
    <ol class="weekly">
      {#each sec.weekly as a, i (a.key)}
        <li>
          <button class="weekly-row" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="n">{i + 1}</span>
            <span class="art tiny"><Cover src={a.cover} /></span>
            <span class="weekly-text"><b>{a.album}</b><small>{a.artist}</small></span>
          </button>
          {#if a.bandcamp}<button class="weekly-play" aria-label={`Play ${a.album} on Bandcamp`} title="Play on Bandcamp" disabled={previewing === a.key} onclick={() => onplay(a)}><Icon name="play" size={17} /></button>{/if}
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  section {
    margin-bottom: 40px;
    container-type: inline-size;
  }

  .art {
    display: block;
    flex: none;
    width: 76px;
    height: 76px;
    overflow: hidden;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .art.tiny {
    width: 40px;
    height: 40px;
  }

  .art.big {
    width: 150px;
    height: 150px;
  }

  .art :global(img),
  .pick-cover :global(img),
  .tile :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  button {
    font: inherit;
    color: inherit;
  }

  small {
    color: var(--muted);
    font-size: 12px;
  }

  .kicker {
    color: var(--accent-ink);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tag {
    padding: 2px 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent-ink);
    font-size: 12px;
  }

  /* The pick: a spread, the cover in proportion to the page and the words beside it; in a narrow container the cover
     the full width on top, the words under it (the owner, 2026-10-25). */
  .pick {
    display: flex;
    flex-wrap: wrap;
    gap: clamp(16px, 4cqi, 32px);
    align-items: center;
  }

  .pick-cover {
    flex: none;
    width: clamp(120px, 34cqi, 340px);
    aspect-ratio: 1;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    box-shadow: 0 20px 50px rgb(0 0 0 / 0.45);
    cursor: pointer;
  }

  /* A narrow container: the cover the full width, and the block under it as wide: its title scaled to the width, Play
     stretched across beside the menu's button (the owner, 2026-10-25). */
  @container (max-width: 440px) {
    .pick-cover {
      width: 100%;
    }

    .pick-text .pick-title {
      font-size: clamp(26px, 8cqi, 40px);
    }

    .pick-acts .play {
      flex: 1;
      justify-content: center;
    }
  }

  .pick-text {
    display: flex;
    flex: 1 1 240px;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .pick-title {
    font-size: clamp(22px, 5cqi, 46px);
    font-weight: 700;
    line-height: 1.05;
    overflow-wrap: anywhere;
  }

  .pick-by {
    color: var(--muted);
    font-size: 18px;
  }

  .pick-facts {
    color: var(--muted);
    font-size: 14px;
  }

  blockquote {
    margin: 6px 0 0;
    padding-left: 14px;
    border-left: 3px solid var(--accent);
    font-size: 16px;
    line-height: 1.5;
  }

  .pick-acts {
    display: flex;
    gap: 10px;
    margin-top: 6px;
  }

  .play {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 42px;
    padding: 0 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
    cursor: pointer;
  }

  .more {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border: 1px solid var(--border);
    border-radius: 50%;
    background: none;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* The masonry: the seeds in the middle, the suggestions around. */
  /* The masonries side by side, each as many columns and rows as it was laid out (lib/masonry); a tile slightly smaller
     than a shelf's cover (the owner, 2026-10-25: "slightly smaller than the discography gaps tiles", 150 px), smaller
     still where three do not fit (a phone). */
  .arounds {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    container-type: inline-size;
  }

  .masonry {
    --tile: min(136px, calc((100cqi - 12px) / 3));
    display: grid;
    grid-template-columns: repeat(var(--cols), var(--tile));
    grid-template-rows: repeat(var(--rows), var(--tile));
    gap: 6px;
  }

  .seeds {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    overflow: hidden;
    padding: 6px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    text-align: center;
  }

  .seed-text {
    display: grid;
    gap: 4px;
  }

  /* With the artist's photo: the words over it at the foot, as a tile's. */
  .seeds.photo {
    justify-content: flex-end;
    padding: 0;
  }

  .seeds img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .seeds.photo .seed-text {
    position: relative;
    width: 100%;
    padding: 18px 7px 6px;
    background: linear-gradient(to top, rgb(0 0 0 / 0.78), transparent);
    color: #fff;
  }

  .seeds.photo small {
    color: rgb(255 255 255 / 0.85);
  }

  .owned {
    color: var(--muted);
    font-size: 11px;
  }

  .seeds.photo .owned {
    color: rgb(255 255 255 / 0.8);
  }

  .seeds small {
    color: var(--accent-ink);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .seeds b {
    font-size: 13px;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  /* A tile's place, holding the tile and its play (a button cannot hold another). */
  .tile-box {
    position: relative;
    display: grid;
  }

  .tile {
    position: relative;
    width: 100%;
    height: 100%;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    cursor: pointer;
  }

  .tile-text {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    display: grid;
    padding: 18px 7px 5px;
    background: linear-gradient(to top, rgb(0 0 0 / 0.78), transparent);
    color: #fff;
    text-align: left;
  }

  .tile-text b,
  .tile-text small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tile-text b {
    font-size: 12px;
  }

  .tile-text small {
    color: rgb(255 255 255 / 0.8);
  }



  /* New to you: the pictures as a flow (as Home's), the front one's name under it, then their biography beside their
     album's card. */
  .artist-flow {
    position: relative;
    height: calc(clamp(180px, 26vw, 260px) + 40px);
  }

  .artist-caption {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin-top: -24px;
    text-align: center;
  }

  .artist-caption .title,
  .artist-caption .by {
    height: 1.4em;
    max-width: 100%;
    overflow: hidden;
    line-height: 1.4;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artist-caption .title {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .artist-caption .by {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .photo-credit {
    color: var(--muted);
    font-size: 11px;
    text-decoration: none;
  }

  .artist-about {
    display: flex;
    gap: 20px;
    align-items: flex-start;
    margin-top: 18px;
  }

  .artist-about .why {
    margin-bottom: 6px;
    color: var(--muted);
    white-space: normal;
  }

  .artist-about p {
    flex: 1;
    min-width: 0;
    margin: 0;
    color: var(--muted);
    font-size: 14px;
    line-height: 1.55;
  }

  /* The fresh releases: a row. */
  .row {
    display: flex;
    gap: 14px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  /* A card and its play over its cover. */
  .album-box {
    position: relative;
    flex: none;
  }

  .album-col {
    display: flex;
    flex: none;
    flex-direction: column;
    width: 150px;
  }

  .album {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 3px;
    width: 150px;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    cursor: pointer;
  }

  .album b {
    margin-top: 6px;
    overflow: hidden;
    font-size: 14px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The Weekly Exploration: a list in two columns where there is room. */
  .weekly {
    margin: 0;
    padding: 0;
    columns: 2 300px;
    column-gap: 28px;
    list-style: none;
  }

  .weekly li {
    display: flex;
    align-items: center;
    gap: 6px;
    break-inside: avoid;
    border-bottom: 1px solid var(--border);
  }

  .weekly-row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 8px 0;
    border: 0;
    background: none;
    text-align: left;
    cursor: pointer;
  }

  .n {
    width: 20px;
    color: var(--muted);
    font-size: 13px;
    text-align: right;
  }

  .weekly-text {
    display: grid;
    min-width: 0;
  }

  .weekly-text b {
    overflow: hidden;
    font-size: 14px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .weekly-play {
    display: grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent-ink);
    cursor: pointer;
  }
</style>
