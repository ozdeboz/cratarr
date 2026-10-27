<script lang="ts">
  // Discover's sections (decision 0069, amended 2026-10-25: "a more dynamic page", each section the form its content
  // suits, inspired by Home's variety, not a copy of it): the pick of the day as a spread; the discography gaps of the
  // artists most played, as a shelf; the masonry around the artists played; the new artists' pictures as a flow;
  // the fresh releases as a row; the Weekly Exploration as the list it is. What
  // plays on Bandcamp comes first everywhere (lib/discoverSections). A tap on a suggestion opens it (`onopen`), a long-press its
  // menu (`onmenu`), as Home's albums; Play hears it on Bandcamp in Cratarr's own player (`onplay`).
  import { untrack } from 'svelte';
  import { albumCoverUrl, discoverArtistPhotoUrl, getDiscoverArtist, type DiscoverAlbum, type RequestMark } from './api';
  import { flowOpened } from './backdrop.svelte';
  import { library } from './albums.svelte';
  import Cover from './Cover.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import CoverBusy from './CoverBusy.svelte';
  import { bandcampFailed, bandcampPlay, playArtistOnBandcamp } from './bandcampFound.svelte';
  import CoverStatus from './CoverStatus.svelte';
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
    problemOf = () => null,
    onproblem = () => {},
    onaroundrefresh = null,
    aroundbusy = false,
    status = (a) => a.request ?? null,
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
    /** What went wrong with an album's request (not handed over): its mark a red "!" (the owner, 2026-10-26). */
    problemOf?: (a: DiscoverAlbum) => string | null;
    /** A red "!" tapped: the request's or the play's, said in a sheet (lib/ProblemSheet). */
    onproblem?: (a: DiscoverAlbum, what: 'request' | 'play') => void;
    /** Around artists' refresh: on to other artists (the owner, 2026-10-26). */
    onaroundrefresh?: (() => void) | null;
    /** The next artists are being made: the refresh waits, the groups shown stay. */
    aroundbusy?: boolean;
    /** Asked for: where it stands, on its cover (lib/CoverStatus). */
    status?: (a: DiscoverAlbum) => RequestMark | null;
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
  /** The pick whose cover is in: another pick's shows the busy round until its picture comes. */
  let pickIn = $state<string | null>(null);
  const albumsOf = (n: number) => `${n} ${n === 1 ? 'album' : 'albums'} in your library`;
  /** The seed's own albums in the library, for its card's fan (a few covers, as Artists to explore's). */
  const ownAlbums = (name: string) => {
    const n = name.toLowerCase();
    return (library.view?.sources ?? []).flatMap((s) => s.albums.filter((a) => a.artist?.toLowerCase() === n).map((a) => albumCoverUrl(s.id, a.id))).slice(0, 3);
  };
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
    <button class="pick-cover" aria-label={`${a.album} by ${a.artist}`} onclick={() => onopen(a)} use:longpress={() => onmenu(a)}
      ><Cover src={a.cover} lazy={false} onready={() => (pickIn = a.key)} /><CoverStatus mark={status(a)} />{#if a.cover && pickIn !== a.key}<CoverBusy />{/if}</button
    >
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
    <div class="row bleed">
      {#each gaps as a (a.key)}
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} onfail={() => (coverless = [...coverless, a.key])} /></span>
            <b>{a.album}</b>
            <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
          </button>
          <CoverStatus mark={status(a)} problem={problemOf(a)} onproblem={() => onproblem(a, 'request')} />{#if a.bandcamp}<CoverPlay bandcamp label={a.album} busy={previewing === a.key} problem={bandcampFailed[a.key] ?? null} onplay={() => onplay(a)} onproblem={() => onproblem(a, 'play')} />{/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<!-- Around the artists you play: the seeds in the middle, their suggestions in a masonry around them. -->
{#if sec.around.length}
  <section aria-label="Around artists you play">
    <SectionHead title={sec.around.length === 1 ? `Around ${[sec.around[0].seeds[0], sec.around[0].partner].filter(Boolean).join(' and ')}` : 'Around artists you play'} onrefresh={onaroundrefresh ?? undefined} busy={aroundbusy} />
    <div class="arounds">
      {#each sec.around as g (g.seeds.join('|'))}
        {@const grid = g.grid}
        {@const name = g.seeds[0]}
        {@const fan = ownAlbums(name)}
        {@const wash = photos[name] ? discoverArtistPhotoUrl(name) : (fan[0] ?? null)}
        <!-- Wider than the page (the owner, 2026-10-26): it scrolls sideways, never leaving empty room. -->
        <div class="around-scroll bleed">
        <div class="masonry" style={`--cols: ${grid.cols}; --rows: ${grid.rows};`}>
          <!-- The seed's own tile (the owner, 2026-10-25), as an artist to explore's card: glass washed in its photo (else
               its first album), its round photo at the top of the left half, its words at the foot of it, a fan of its
               albums in the right half. -->
          <div class="seeds seed-card" style={place(grid.seed)}>
            {#if wash}<div class="wash" aria-hidden="true"><img src={wash} alt="" onerror={() => photos[name] && (photos[name] = false)} /></div>{/if}
            <!-- "Because you play" at the top left, the round photo under it taking the rest of the column, the name and how
                 many albums at its foot (the owner, 2026-10-26). -->
            <div class="seed-left" class:photo={photos[name]}>
              <small class="because">Because you play</small>
              {#if photos[name]}<span class="face-room"><img class="face" src={discoverArtistPhotoUrl(name)} alt="" /></span>{/if}
              <span class="seed-text"><b>{name}</b>{#if owned(name)}<span class="owned">{albumsOf(owned(name))}</span>{/if}</span>
            </div>
            {#if fan.length}<div class="fan" aria-hidden="true">{#each [...fan].reverse() as src (src)}<img {src} alt="" />{/each}</div>{/if}
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
                <CoverStatus mark={status(a)} problem={problemOf(a)} onproblem={() => onproblem(a, 'request')} />{#if a.bandcamp}<CoverPlay bandcamp label={a.album} busy={previewing === a.key} problem={bandcampFailed[a.key] ?? null} onplay={() => onplay(a)} onproblem={() => onproblem(a, 'play')} />{/if}
              </div>
            {/if}
          {/each}
        </div>
        </div>
      {/each}
    </div>
  </section>
{/if}

<!-- New to you (the owner, 2026-10-25; laid out anew 2026-10-27, mock B): the new artists' pictures as a flow (Home's,
     lib/CoverFlow), a picture opening its artist; over the flow's foot the front one's name, their studio albums and
     Listen (all they have on Bandcamp, lib/bandcampFound); below, a short piece of their biography with their album to
     its right. -->
{#if sec.newArtists.length}
  {@const n = sec.newArtists[Math.min(front, sec.newArtists.length - 1)]}
  {@const a = n.items[0]}
  {@const listenKey = `artist:${n.artist}`}
  <section aria-label="New to you">
    <SectionHead title="New to you" />
    <div class="artist-flow" {@attach () => untrack(() => flowOpened('discover:new'))}>
      <CoverFlow
        covers={sec.newArtists.map((x) => discoverArtistPhotoUrl(x.artist))}
        top={0}
        foot={40}
        onfront={(i) => (front = i)}
        onopen={(i) => goToArtist(DISCOVER, sec.newArtists[i].artist)}
        onmenu={(i) => onmenu(sec.newArtists[i].items[0])}
      />
      <div class="artist-over">
        <div class="artist-name">
          <button class="title" onclick={() => goToArtist(DISCOVER, n.artist)}>{n.artist}</button>
          <!-- Their studio albums (the owner, 2026-10-25: "the golden standard"), as the look counted. -->
          {#if a.artist_albums}<span class="by">{a.artist_albums} studio {a.artist_albums === 1 ? 'album' : 'albums'}</span>{/if}
        </div>
        <button
          class="play"
          disabled={bandcampPlay.key === listenKey}
          aria-busy={bandcampPlay.key === listenKey}
          onclick={() => playArtistOnBandcamp(listenKey, n.artist, a.bandcamp ?? null)}><Icon name="play" size={18} />Listen</button
        >
      </div>
    </div>
    {#if bandcampFailed[listenKey]}<p class="artist-failed">{bandcampFailed[listenKey]}</p>{/if}
    <div class="artist-about">
      <div class="about-text">
        {#if bio?.name === n.artist && bio.text}<p>{bio.text}</p>{/if}
        {#if bio?.name === n.artist && bio.credit}<a class="photo-credit" href={bio.credit.url} target="_blank" rel="noopener noreferrer">Photo: {bio.credit.author} · {bio.credit.licence}</a>{/if}
      </div>
      <div class="album-col">
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} /></span>
            <b>{a.album}</b>
            <small>{a.year ?? ''}</small>
          </button>
          <CoverStatus mark={status(a)} problem={problemOf(a)} onproblem={() => onproblem(a, 'request')} />{#if a.bandcamp}<CoverPlay bandcamp label={a.album} busy={previewing === a.key} problem={bandcampFailed[a.key] ?? null} onplay={() => onplay(a)} onproblem={() => onproblem(a, 'play')} />{/if}
        </div>
        {#if a.why.length}<small class="why">{a.why.join('. ')}</small>{/if}
      </div>
    </div>
  </section>
{/if}

<!-- New from artists you have: the fresh releases, newest first. -->
{#if sec.fresh.length}
  <section aria-label="New from artists you have">
    <SectionHead title="New from artists you have" />
    <div class="row bleed">
      {#each sec.fresh as a (a.key)}
        <div class="album-box">
          <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
            <span class="art big"><Cover src={a.cover} /></span>
            <b>{a.album}</b>
            <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
          </button>
          <CoverStatus mark={status(a)} problem={problemOf(a)} onproblem={() => onproblem(a, 'request')} />{#if a.bandcamp}<CoverPlay bandcamp label={a.album} busy={previewing === a.key} problem={bandcampFailed[a.key] ?? null} onplay={() => onplay(a)} onproblem={() => onproblem(a, 'play')} />{/if}
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
            <span class="art tiny"><Cover src={a.cover} /><CoverStatus mark={status(a)} small /></span>
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
    position: relative;
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
    position: relative;
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
  /* Each artist's grid its own row, wider than the page: it scrolls sideways (no bar), past the page's padding. */
  .arounds {
    display: grid;
    gap: 24px;
    container-type: inline-size;
  }

  .around-scroll {
    overflow-x: auto;
    scrollbar-width: none;
    overscroll-behavior-x: contain;
  }

  .around-scroll::-webkit-scrollbar {
    display: none;
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

  /* The seed's card (Artists to explore's look, lib/ArtistsToExplore): glass washed in its photo, a dark veil over
     it; the left half the round photo over the words, the right half the fanned covers. */
  .seeds.seed-card {
    isolation: isolate;
    display: grid;
    grid-template-columns: 1fr 1fr;
    align-items: stretch;
    justify-content: stretch;
    padding: 10px;
    border: 0;
    border-radius: 12px;
    background: var(--surface);
    color: #fff;
    box-shadow: inset 0 0 0 1px var(--island-ring, rgb(255 255 255 / 0.08));
  }

  .seed-card .wash {
    position: absolute;
    inset: -30%;
    z-index: -2;
    filter: blur(36px) saturate(1.6);
    opacity: 0.8;
  }

  .seed-card .wash img {
    position: static;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .seed-card::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(180deg, rgb(10 12 16 / 0.3), rgb(10 12 16 / 0.7));
  }

  .seed-left {
    position: relative;
    display: grid;
    grid-template-rows: auto 1fr;
    justify-items: start;
    min-width: 0;
    min-height: 0;
    gap: 4px;
  }

  .seed-left .because {
    font-size: 11px;
  }

  /* The photo's room: the rest of the column, the round photo as tall as it, the name and count over its foot. */
  .face-room {
    position: relative;
    min-height: 0;
    height: 100%;
    max-width: 100%;
    aspect-ratio: 1;
  }

  .seed-card .face {
    position: static;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    box-shadow: 0 4px 12px rgb(0 0 0 / 0.45);
  }

  .seed-card .seed-text {
    align-self: end;
    text-align: start;
  }

  .seed-left.photo .seed-text {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    text-shadow: 0 1px 4px rgb(0 0 0 / 0.8);
  }

  .seed-card .seed-text b {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .seed-card small,
  .seed-card .owned {
    color: rgb(255 255 255 / 0.8);
  }

  /* Its albums fanned, as Artists to explore's, sized to the half. */
  .seed-card .fan {
    position: relative;
    min-width: 0;
  }

  .seed-card .fan img {
    position: absolute;
    top: 50%;
    width: min(64%, 84px);
    height: auto;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 6px;
    box-shadow: 0 4px 12px rgb(0 0 0 / 0.5);
  }

  .seed-card .fan img:nth-child(1) {
    left: 6%;
    transform: translateY(-50%) rotate(-9deg);
  }

  .seed-card .fan img:nth-child(2) {
    left: 20%;
    transform: translateY(-50%) rotate(-3deg);
  }

  .seed-card .fan img:nth-child(3) {
    left: 34%;
    transform: translateY(-50%) rotate(4deg);
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



  /* New to you: the pictures as a flow (as Home's), the front one's name and Listen over its foot, then a piece of their
     biography beside their album's card. */
  .artist-flow {
    position: relative;
    height: calc(clamp(180px, 26vw, 260px) + 40px);
  }

  /* One's own picture behind every flow, under a veil (Settings › Appearance; the owner, 2026-10-26). */
  :global(:root[data-flow-photo]) .artist-flow {
    background:
      linear-gradient(to bottom, rgb(0 0 0 / 0.35), rgb(0 0 0 / 0.55)),
      var(--flow-photo) center 20% / cover no-repeat,
      var(--bg);
    border-radius: 12px;
  }

  /* Over the flow's foot (mock B, the owner 2026-10-27): the front artist's name as a title and Listen, on a shade so
     they read over the pictures. */
  .artist-over {
    position: absolute;
    inset: auto 0 0;
    z-index: 2;
    display: flex;
    gap: 12px;
    align-items: flex-end;
    justify-content: space-between;
    padding: 48px 18px 14px;
    border-radius: 0 0 12px 12px;
    background: linear-gradient(to bottom, transparent, color-mix(in srgb, var(--bg) 80%, transparent));
    pointer-events: none;
  }

  .artist-over > * {
    pointer-events: auto;
  }

  .artist-name {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .artist-name .title,
  .artist-name .by {
    max-width: 100%;
    overflow: hidden;
    line-height: 1.3;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artist-name .title {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 1.35rem;
    font-weight: 600;
    cursor: pointer;
  }

  .artist-name .by {
    color: color-mix(in srgb, var(--text) 75%, transparent);
    font-size: 0.9rem;
  }

  .artist-over .play {
    flex: none;
  }

  .artist-failed {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: 13px;
  }

  .photo-credit {
    display: inline-block;
    margin-top: 6px;
    color: var(--muted);
    font-size: 11px;
    text-decoration: none;
  }

  .artist-about {
    display: flex;
    gap: 20px;
    align-items: flex-start;
    margin-top: 16px;
  }

  .about-text {
    flex: 1;
    min-width: 0;
  }

  .artist-about .why {
    margin-top: 6px;
    color: var(--muted);
    white-space: normal;
  }

  /* A piece of their biography, not all of it (the owner, 2026-10-27): its first four lines. */
  .artist-about p {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 14px;
    line-height: 1.55;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
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
