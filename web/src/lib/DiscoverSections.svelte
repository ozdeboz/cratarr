<script lang="ts">
  // Discover's sections (decision 0069, amended 2026-10-25: "a more dynamic page", each section the form its content
  // suits, inspired by Home's variety, not a copy of it): the pick of the day as a spread; the discography gaps of the
  // artists most played, as a shelf; the masonry around the artists played; one new artist
  // in the light, the others as chips; the fresh releases as a row; the Weekly Exploration as the list it is. What
  // plays on Bandcamp comes first everywhere (lib/discoverSections). A tap on a suggestion opens it (`onopen`), a long-press its
  // menu (`onmenu`), as Home's albums; Play hears it on Bandcamp in Cratarr's own player (`onplay`).
  import { discoverArtistPhotoUrl, getDiscoverArtist, type DiscoverAlbum } from './api';
  import Cover from './Cover.svelte';
  import type { Sections } from './discoverSections';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SectionHead from './SectionHead.svelte';

  let {
    sec,
    facts,
    onopen,
    onmenu,
    onplay,
    previewing = null,
  }: {
    sec: Sections<DiscoverAlbum>;
    /** A suggestion's facts in one line (its type, tracks, length, listeners). */
    facts: (a: DiscoverAlbum) => string;
    onopen: (a: DiscoverAlbum) => void;
    /** Its menu: a long-press, or the pick's More. */
    onmenu: (a: DiscoverAlbum) => void;
    onplay: (a: DiscoverAlbum) => void;
    previewing?: string | null;
  } = $props();

  // New to you: the artist in the light, the first of the new artists with a picture (the owner, 2026-10-25: one
  // without is never featured; the pictures and descriptions come with the sync); a few lines about them
  // (TheAudioDB's, as the sync kept it); none with a picture: the chips alone.
  const lit = $derived(sec.newArtists.find((n) => n.items.some((a) => a.artist_photo)) ?? null);
  let bio = $state<{ name: string; text: string | null } | null>(null);
  $effect(() => {
    const name = lit?.artist;
    if (!name || bio?.name === name) return;
    getDiscoverArtist(name).then(
      (a) => (bio = { name, text: a.bio }),
      () => (bio = { name, text: null }),
    );
  });
  const others = $derived(sec.newArtists.filter((n) => n.artist !== lit?.artist));
  /** A biography's first lines: at most a few sentences. */
  const short = (bio: string) => {
    const s = bio.split(/(?<=[.!?])\s+/).slice(0, 3).join(' ');
    return s.length > 420 ? `${s.slice(0, 417)}…` : s;
  };

  // The gaps whose covers came (Cover's onfail: none to show, so not shown).
  let coverless = $state<string[]>([]);
  const gaps = $derived(sec.gaps.filter((a) => a.cover && !coverless.includes(a.key)));

  // The masonry (the owner, 2026-10-25: "because you listen to this, and then 8 tiles around that, or fewer"): a grid
  // of three by three, the seeds in the middle, the suggestions in the eight places around them, always all filled, in
  // turn from the top left; what plays on Bandcamp a tile of one by two, the rest one by one (the owner, 2026-10-25). Places numbered 1
  // to 9 as read, 5 the middle; any two following each other around it are side by side.
  const RING = [1, 2, 3, 6, 9, 8, 7, 4];
  const places = (g: { items: { key: string }[]; wide: string[] }) => {
    let at = 0;
    return g.items.map((s) => {
      const cells = RING.slice(at, (at += g.wide.includes(s.key) ? 2 : 1));
      const cols = cells.map((c) => ((c - 1) % 3) + 1);
      const rows = cells.map((c) => Math.floor((c - 1) / 3) + 1);
      const span = (v: number[]) => `${Math.min(...v)} / span ${Math.max(...v) - Math.min(...v) + 1}`;
      return `grid-column: ${span(cols)}; grid-row: ${span(rows)};`;
    });
  };
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
        <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
          <span class="art big"><Cover src={a.cover} onfail={() => (coverless = [...coverless, a.key])} /></span>
          <b>{a.album}</b>
          <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
        </button>
      {/each}
    </div>
  </section>
{/if}

<!-- Around the artists you play: the seeds in the middle, their suggestions in a masonry around them. -->
{#each sec.around as g (g.seeds.join('|'))}
  {@const at = places(g)}
  <section aria-label={`Around ${g.seeds.join(' and ')}`}>
    <SectionHead title={`Around ${g.seeds.join(' and ')}`} />
    <div class="masonry-box"><div class="masonry">
      <div class="seeds"><small>Because you play</small><b>{g.seeds.join(' · ')}</b></div>
      {#each g.items as a, i (a.key)}
        <button class="tile" style={at[i]} aria-label={`${a.album} by ${a.artist}`} onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
          <Cover src={a.cover} />
          <span class="tile-text"><b>{a.album}</b><small>{a.artist}</small></span>
          {#if a.bandcamp}<span class="tile-play" aria-hidden="true"><Icon name="play" size={14} /></span>{/if}
        </button>
      {/each}
    </div></div>
  </section>
{/each}

<!-- New to you: one new artist in the light, the others as chips. -->
{#if lit}
  {@const a = lit.items[0]}
  <section class="spot" aria-label="New to you">
    <span class="spot-photo"><img src={discoverArtistPhotoUrl(lit.artist)} alt="" /></span>
    <div class="spot-text">
      <span class="kicker">New to you</span>
      <span class="spot-name">{lit.artist}</span>
      {#if bio?.name === lit.artist && bio.text}<p>{short(bio.text)}</p>{/if}
      <button class="spot-album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}
        ><span class="art small"><Cover src={a.cover} /></span><span><b>{a.album}</b><small>{a.why[0] ?? ''}</small></span></button
      >
      {#if others.length}
        <span class="chips">{#each others as n (n.artist)}<button class="chip" onclick={() => onopen(n.items[0])} use:longpress={() => onmenu(n.items[0])}>{n.artist}</button>{/each}</span>
      {/if}
    </div>
  </section>
{:else if others.length}
  <section aria-label="New to you">
    <SectionHead title="New to you" />
    <span class="chips">{#each others as n (n.artist)}<button class="chip" onclick={() => onopen(n.items[0])} use:longpress={() => onmenu(n.items[0])}>{n.artist}</button>{/each}</span>
  </section>
{/if}

<!-- New from artists you have: the fresh releases, newest first. -->
{#if sec.fresh.length}
  <section aria-label="New from artists you have">
    <SectionHead title="New from artists you have" />
    <div class="row">
      {#each sec.fresh as a (a.key)}
        <button class="album" onclick={() => onopen(a)} use:longpress={() => onmenu(a)}>
          <span class="art big"><Cover src={a.cover} /></span>
          <b>{a.album}</b>
          <small>{a.artist}{a.year ? ` · ${a.year}` : ''}</small>
        </button>
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
          {#if a.bandcamp}<button class="weekly-play" aria-label={`Play ${a.album} on Bandcamp`} title="Play on Bandcamp" disabled={previewing === a.key} onclick={() => onplay(a)}><Icon name="play" size={16} /></button>{/if}
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

  .art.small {
    width: 52px;
    height: 52px;
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
  /* Half as wide and as tall as it was (the owner, 2026-10-25): a tile about the size of a shelf's cover. */
  .masonry-box {
    max-width: 360px;
    container-type: inline-size;
  }

  .masonry {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    grid-auto-rows: calc((100cqi - 12px) / 3);
    gap: 6px;
  }

  .seeds {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 4px;
    grid-column: 2;
    grid-row: 2;
    padding: 6px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    text-align: center;
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

  .tile {
    position: relative;
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

  .tile-play {
    position: absolute;
    top: 5px;
    right: 5px;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
  }


  /* New to you: one artist in the light. */
  .spot {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    align-items: flex-start;
  }

  .spot-photo {
    display: grid;
    flex: none;
    place-items: center;
    width: 160px;
    height: 160px;
    overflow: hidden;
    border-radius: 50%;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--muted);
  }

  .spot-photo img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .spot-text {
    display: flex;
    flex: 1 1 360px;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .spot-name {
    font-size: 30px;
    font-weight: 700;
  }

  .spot-text p {
    margin: 0;
    color: var(--text);
    font-size: 15px;
    line-height: 1.55;
  }

  .spot-album {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px;
    border: 0;
    border-radius: 10px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
    text-align: left;
    cursor: pointer;
  }

  .spot-album span:last-child {
    display: grid;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    height: 34px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    font-size: 13px;
    cursor: pointer;
  }

  /* The fresh releases: a row. */
  .row {
    display: flex;
    gap: 14px;
    overflow-x: auto;
    scrollbar-width: none;
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
