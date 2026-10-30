<script lang="ts">
  // An artist's page under their name (decided 2026-10-16, as Symfonium's): About (the owner, 2026-10-23: TheAudioDB's
  // description, from the server's artist store; a tap opens all of it; their photo is the page's header, not here too), their albums, the albums they
  // appear on (others', credited on a track), both newest first as covers, and their most played tracks (the panel's
  // rows, `top`), whose › shows all their tracks; then related artists to explore (lib/RelatedArtists, 0.9.2); last
  // their albums not in the library (0.9.5).
  import ProblemSheet from './ProblemSheet.svelte';
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import { getArtistAbout, getArtistDiscogs, getArtistMissing, getDiscoverArtist, type ArtistDiscogs, type ListedAlbum, type MissingAlbum, type PhotoCredit } from './api';
  import { cameFrom, DISCOVER } from './discoverArtist';
  import { pageAsked } from './artistAbout.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import RelatedArtists from './RelatedArtists.svelte';
  import ElsewhereAlbum from './ElsewhereAlbum.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import { bandcampOf, bandcampPlay, playOnBandcamp, bandcampFailed } from './bandcampFound.svelte';
  import SectionHead from './SectionHead.svelte';
  import { matchesSearch } from './pageView.svelte';

  let {
    source,
    albumSource = source,
    name,
    mbid = null,
    albums,
    appearsOn,
    top,
    ontracks,
    profile,
    profileComing = false,
    searching = false,
  }: {
    source: string;
    /** Where the albums open (an artist from Discover: Bandcamp's). */
    albumSource?: string;
    name: string;
    /** Their MusicBrainz id, where known: Discogs is found by it. */
    mbid?: string | null;
    albums: ListedAlbum[];
    appearsOn: ListedAlbum[];
    top?: Snippet;
    ontracks: () => void;
    profile?: Snippet;
    /** The profile is on its way: its column is kept from the start, so the albums do not narrow when it comes. */
    profileComing?: boolean;
    /** The panel's own search is on (the owner, 2026-10-25): only hits; the about if it has the words (whole), the
     *  albums that have them, no profile, no related artists (they lead off the panel). */
    searching?: boolean;
  } = $props();
  const hit = (...texts: (string | null | undefined)[]) => !searching || matchesSearch(...texts);
  const albumsShown = $derived(albums.filter((a) => hit(a.title, a.artist)));
  const appearsShown = $derived(appearsOn.filter((a) => hit(a.title, a.artist)));

  // About: Cratarr's own library only (the store knows its artists).
  let about = $state<{ id: string | null; bio: string | null; photo: boolean; photo_credit?: PhotoCredit | null } | null>(null);
  let whole = $state(false);
  $effect(() => {
    const [s, n] = [source, name];
    let gone = false;
    about = null;
    whole = false;
    // An artist from Discover (lib/discoverArtist): their words as New to you has them.
    if (s === DISCOVER)
      getDiscoverArtist(n).then(
        (a) => !gone && (about = { id: null, bio: a.bio, photo: a.photo, photo_credit: a.photo_credit }),
        () => {},
      );
    if (s === 'cratarr')
      getArtistAbout(n).then(
        async (a) => {
          if (gone) return;
          about = a;
          // Asked for from Home's description: unfolded, and in view.
          if (pageAsked.name === n && pageAsked.section === 'about') {
            pageAsked.name = null;
            whole = true;
            await tick();
            aboutEl?.scrollIntoView({ block: 'start', behavior: 'smooth' });
          }
        },
        () => {},
      );
    return () => (gone = true);
  });
  // Not in your library (the owner, 2026-10-25; the server's lookup since 0.9.3): the artist's albums MusicBrainz lists
  // that the library lacks, asked when the page opens; while the server is still reading their discography (`looking`)
  // asked again a little later. A tap opens one on Bandcamp, else its menu (Want; lib/ElsewhereAlbum). Cratarr's own
  // library's artists, and an artist from Discover (the owner, 2026-10-26: whatever Discover suggests today), found by
  // the Bandcamp album the page was opened from where its release group is known.
  let missing = $state<MissingAlbum[]>([]);
  $effect(() => {
    const [s, n] = [source, name];
    missing = [];
    if (s !== 'cratarr' && s !== DISCOVER) return;
    const hint = s === DISCOVER && cameFrom.name?.toLowerCase() === n.toLowerCase() ? { release_group: cameFrom.album?.release_group ?? null } : {};
    let gone = false;
    let t = 0;
    const ask = (left: number) =>
      getArtistMissing(n, hint).then(
        (r) => {
          if (gone) return;
          missing = r.albums;
          if (r.looking && left > 0) t = window.setTimeout(() => ask(left - 1), 4000);
        },
        () => {},
      );
    void ask(5);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  const missingShown = $derived(missing.filter((a) => hit(a.title)));
  let elsewhere = $state<ElsewhereAlbum>();
  let finding = $state<string | null>(null);
  let asked = $state<string[]>([]);
  let aboutEl = $state<HTMLElement>();

  // Discogs (0.9.8, the owner: "Yes to all"): their profile where TheAudioDB has none, and who they are there: their
  // real name, members, the groups they are in, the names they also go by, and their page. Cratarr's own library only.
  let discogs = $state<ArtistDiscogs | null>(null);
  $effect(() => {
    const [s, n, m] = [source, name, mbid];
    discogs = null;
    if (s !== 'cratarr') return;
    let gone = false;
    getArtistDiscogs(n, m).then(
      (d) => !gone && (discogs = d),
      () => {},
    );
    return () => (gone = true);
  });
  const bio = $derived(about?.bio || discogs?.profile || null);
  const bioFrom = $derived(about?.bio ? 'TheAudioDB' : discogs?.profile ? 'Discogs' : null);
  /** Who they are on Discogs, each a line of names (a former member or group muted). */
  const facts = $derived.by(() => {
    const d = discogs;
    if (!d) return [];
    const out: { label: string; names: { name: string; past: boolean }[] }[] = [];
    if (d.real_name && d.real_name !== name) out.push({ label: 'Real name', names: [{ name: d.real_name, past: false }] });
    if (d.members?.length) out.push({ label: 'Members', names: d.members.map((m) => ({ name: m.name, past: !m.active })) });
    if (d.groups?.length) out.push({ label: 'In', names: d.groups.map((m) => ({ name: m.name, past: !m.active })) });
    if (d.aliases?.length) out.push({ label: 'Also as', names: d.aliases.map((a) => ({ name: a, past: false })) });
    return out;
  });
  // Asked for from Home's fanned albums: Albums in view (once the page above it has its height).
  let albumsEl = $state<HTMLElement>();
  $effect(() => {
    if (!albumsEl || pageAsked.name !== name || pageAsked.section !== 'albums') return;
    pageAsked.name = null;
    const el = albumsEl;
    setTimeout(() => el.scrollIntoView({ block: 'start', behavior: 'smooth' }), 400);
  });
  /** A play that failed, said in its sheet (its button a red "!"; the owner, 2026-10-26). */
  let playProblem = $state<{ title: string; text: string; retry: () => void } | null>(null);
</script>

{#snippet grid(list: ListedAlbum[])}
  <ul class="grid">
    {#each list as a (a.id)}<li><AlbumCard source={albumSource} album={a} /></li>{/each}
  </ul>
{/snippet}

{#if searching ? !!bio && hit(bio) : !!(bio || about?.photo_credit || facts.length || discogs?.url)}
  <section aria-label="About" bind:this={aboutEl}>
    <SectionHead title="About" />
    <div class="about">
      {#if bio}
        <button class="bio" class:whole={whole || searching} onclick={() => (whole = !whole)} aria-expanded={whole || searching}>{bio}</button>
      {/if}
    </div>
    {#if bioFrom}<small class="credit">{bioFrom}</small>{/if}
    <!-- A free photo's credit, as its licence asks (Wikimedia Commons', 0.9.3). -->
    {#if about?.photo_credit}<small class="credit"><a href={about.photo_credit.url} target="_blank" rel="noopener noreferrer">Photo: {about.photo_credit.author} · {about.photo_credit.licence}</a></small>{/if}
    {#if !searching && (facts.length || discogs?.url)}
      <div class="facts">
        {#each facts as f (f.label)}
          <h3>{f.label}</h3>
          <p class="chips">{#each f.names as x (x.name)}<span class="chip" class:past={x.past}>{x.name}</span>{/each}</p>
        {/each}
        {#if discogs?.url}
          <h3>Elsewhere</h3>
          <p class="chips"><a class="chip" href={discogs.url} target="_blank" rel="noopener noreferrer">Discogs</a></p>
        {/if}
      </div>
    {/if}
  </section>
{/if}
<!-- On a wide panel the albums and the artist's profile side by side, the profile all open (the owner, 2026-10-23); on
     a narrow one the profile stays in the header, to fold. -->
<div class="beside" class:with-profile={!searching && (!!profile || profileComing)}>
  {#if albumsShown.length}
    <section aria-label="Albums" bind:this={albumsEl}>
      <SectionHead title="Albums" />
      {@render grid(albumsShown)}
    </section>
  {/if}
  {#if searching}<!-- no profile while searching -->{:else if profile}<aside class="profile" aria-label="Profile">{@render profile()}</aside>{:else if profileComing}<aside class="profile" aria-hidden="true"></aside>{/if}
</div>
{#if appearsShown.length}
  <section aria-label="Appears on">
    <SectionHead title="Appears on" />
    {@render grid(appearsShown)}
  </section>
{/if}
{#if top}
  <section aria-label="Top recommendations">
    <SectionHead title="Top recommendations" onmore={ontracks} />
    {@render top()}
  </section>
{/if}
<!-- Related artists to explore (0.9.2): Cratarr's own library only (the server knows its artists' MusicBrainz ids). -->
{#if !searching && (source === 'cratarr' || source === DISCOVER)}<RelatedArtists {name} />{/if}
<!-- Their albums the library lacks, last on the page (the owner, 2026-10-25: "at the very bottom"). -->
{#if missingShown.length}
  <section aria-label="Not in your library">
    <SectionHead title="Not in your library" />
    <ul class="grid">
      {#each missingShown as m (m.release_group)}
        {@const page = bandcampOf(m.release_group, m.bandcamp)}
        <li class="missing">
          <AlbumCard
            source="musicbrainz"
            album={{ id: m.release_group, title: m.title, year: m.year }}
            cover={m.cover}
            busy={finding === m.release_group}
            request={m.request ?? (asked.includes(m.release_group) ? { state: 'wanted' } : null)}
            onopen={() => elsewhere?.open({ release_group: m.release_group, title: m.title, artist: m.artist, year: m.year, cover: m.cover, bandcamp: page })}
          />
          <!-- On Bandcamp: Play on its cover, as Discover's cards have it (beside the card's button, over its cover). -->
          {#if page}<CoverPlay bandcamp
              label={m.title}
              busy={bandcampPlay.key === m.release_group}
              problem={bandcampFailed[m.release_group] ?? null}
              onplay={() => playOnBandcamp(m.release_group, page, m.title)}
              onproblem={() => (playProblem = { title: `${m.title} did not play`, text: bandcampFailed[m.release_group], retry: () => playOnBandcamp(m.release_group, page, m.title) })}
            />{/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
<ElsewhereAlbum bind:this={elsewhere} bind:finding bind:asked />
{#if playProblem}{@const x = playProblem}<ProblemSheet title={x.title} text={x.text} retry={x.retry} onclose={() => (playProblem = null)} />{/if}
{#if searching && !albumsShown.length && !appearsShown.length && !top && !(bio && hit(bio))}
  <p class="empty">Nothing of {name}'s matches.</p>
{:else if !albums.length && !appearsOn.length && !top}
  <p class="empty">Nothing by {name} on this server.</p>
{/if}

<style>
  /* A missing album's card with Play over its cover (lib/CoverPlay is placed by it). */
  .missing {
    position: relative;
  }

  section {
    margin-top: 16px;
  }

  .beside > .profile {
    display: none;
  }

  @container (min-width: 640px) {
    .beside.with-profile {
      display: grid;
      grid-template-columns: minmax(0, 1fr) clamp(280px, 45cqi, 480px);
      align-items: start;
      column-gap: 24px;
    }

    .beside > .profile {
      display: block;
      margin-top: 16px;
      padding-right: 16px;
    }

    /* Beside the albums nothing is above it: no line over it (the owner, 2026-10-24). */
    .beside > .profile :global(.profile > .rule) {
      display: none;
    }
  }

  .about {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 4px 16px 0;
  }

  /* Four lines; a tap shows all of it (its paragraphs as written). */
  .bio {
    display: -webkit-box;
    overflow: hidden;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 0.95rem;
    line-height: 1.5;
    text-align: left;
    white-space: pre-line;
    cursor: pointer;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
  }

  .bio.whole {
    display: block;
  }

  /* Who they are on Discogs: as an album's About writes its facts (lib/AlbumAbout). */
  .facts {
    padding: 0 16px;
  }

  .facts h3 {
    margin: 14px 0 8px;
    color: var(--muted);
    font-size: 17px;
    font-weight: 400;
  }

  .facts .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding-left: 8px;
  }

  .facts .chip {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: 8px;
    color: inherit;
    font-size: 14px;
    text-decoration: none;
  }

  .facts .chip.past {
    color: var(--muted);
  }

  .credit {
    display: block;
    padding: 4px 16px 0;
    color: var(--muted);
    font-size: 11px;
    text-align: right;
  }

  /* Three across on a phone, more as the panel widens. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: 20px 12px;
    margin: 0;
    padding: 4px 16px 0;
    list-style: none;
  }

  li {
    min-width: 0;
  }

  .empty {
    padding: 24px 16px;
    color: var(--muted);
  }
</style>
