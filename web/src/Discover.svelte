<script lang="ts">
  // Discover (decision 0069; the owner, 2026-10-24: "suggestions outside of" the library, "a discover page next to
  // playlists"): albums the library does not have, from its artists' discographies and the signed-in account's
  // ListenBrainz and Last.fm (0.9.1: every account, a service or none), each with why
  // it is suggested, a cover, where to listen elsewhere, Want (a request, decision 0070: Lidarr fetches it into Incoming
  // once approved, where it is heard and kept or deleted; the card says how far it is) and Not interested (it never
  // comes back). Everything known of it is on its card (the owner, 2026-10-24: what it is, its genres, length and track
  // count, why, whether the artist is new, its listeners, its request); every action in its menu, opened by a tap. Looked up again once a day as a task, its progress in
  // the pill (ui-conventions, A page's work in its pill); Look again there asks again now. Without a service connected,
  // a line says what one would add. In modes (the owner, 2026-10-29), picked beside back (lib/DiscoverPicker): Fresh,
  // Alike, Gaps and New artists, each suggestion in the one whose reason fits it best, and the account's Spotify lists.
  import ProblemSheet from './lib/ProblemSheet.svelte';
  import { getDiscover, hideDiscover, notDiscover, notDiscoverArtist, refreshDiscover, wantDiscover, type AlbumRequest, type DiscoverAlbum, type DiscoverView } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import CoverPlay from './lib/CoverPlay.svelte';
  import CoverStatus from './lib/CoverStatus.svelte';
  import { markOf } from './lib/requests.svelte';
  import { hoursMinutes } from './lib/duration';
  import MenuCards from './lib/MenuCards.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuGroup from './lib/MenuGroup.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import LinkSheet from './lib/LinkSheet.svelte';
  import { goToArtist } from './lib/albums.svelte';
  import { DISCOVER } from './lib/discoverArtist';
  import { NearBlocks } from './lib/nearBlocks.svelte';
  import { inModeOrder, isSpotifyView, modeOf, type ReasonMode } from './lib/discoverModes';
  import { prefs } from './lib/prefs.svelte';
  import SpotifyLibrary from './lib/SpotifyLibrary.svelte';
  import { inSpotify, loadSpotify, spotify } from './lib/spotify.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import { withUndo } from './lib/undo.svelte';
  import { longpress } from './lib/longpress';
  import { albumSubject, viewOver } from './lib/panel.svelte';
  import { BANDCAMP, albumIdOf } from './lib/bandcampPreview';
  import Meter from './lib/Meter.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { matchesSearch, pageView, usePageAction } from './lib/pageView.svelte';
  import { switchOn } from './lib/listFilter.svelte';
  import { progressOf, tasks } from './lib/tasks.svelte';
  import { me } from './lib/me.svelte';
  import { bandcampPlay, playOnBandcamp, bandcampFailed } from './lib/bandcampFound.svelte';
  import { toast } from './lib/toast.svelte';

  // Want: a request for everyone who may ask (decision 0070); an administrator's (or Auto-approve) is handed to Lidarr
  // at once.

  const KIND = 'discover';
  let view = $state<DiscoverView | null>(null);
  let error = $state<string | null>(null);
  const load = () =>
    getDiscover().then(
      (v) => ((view = v), (error = null)),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  load();

  // The look under way (the server's task), followed while the page shows; when it ends, the list is read again.
  $effect(() => tasks.follow());
  const task = $derived(tasks.of(KIND));
  const looking = $derived(tasks.running(KIND));
  const canWant = $derived(!!view?.may_request);
  let seenEnd = $state<number | null>(null);
  $effect(() => {
    const t = task;
    if (t && t.finished !== null && t.id !== seenEnd) {
      seenEnd = t.id;
      // A look asked for by hand (Look again) always ends in a toast, whatever it came to (the owner, 2026-10-24).
      const said = askedLook;
      askedLook = false;
      if (t.error) toast.show({ text: t.error, tone: 'error' });
      load().then(() => {
        if (!said || t.error) return;
        const n = view?.found?.albums.length ?? 0;
        toast.show({ text: n ? `Found ${n} ${n === 1 ? 'suggestion' : 'suggestions'}` : 'Nothing new to suggest', tone: 'done' });
      });
    }
  });
  const progress = $derived(looking ? progressOf(task) : null);
  // Shown the moment they are known (the owner, 2026-10-24, 0.9): the server keeps the list before it looks up where to
  // listen (server discover::LISTEN_STEP), and again every ten albums; read again then, so the list shows at once and
  // fills in behind its progress line at the top.
  const LISTEN_STEP = 'Finding where to listen';
  let readAt = -1;
  $effect(() => {
    const s = looking ? task?.step : null;
    if (!s || s.name !== LISTEN_STEP) {
      readAt = -1;
      return;
    }
    if (readAt < 0 || s.done >= readAt + 10) {
      readAt = s.done;
      load();
    }
  });

  /** Look again was tapped: its end is said in a toast. */
  let askedLook = false;
  async function refresh() {
    try {
      askedLook = true;
      tasks.begun((await refreshDiscover()).task);
    } catch (e) {
      toast.fail(e, 'Could not look again');
    }
  }
  const connected = $derived(!!view && (view.services.listenbrainz || view.services.lastfm));
  // Every account looks (the library's discographies need no service, 0.9.1); the button is there from the start, so
  // the pill does not grow again once the list has come.
  usePageAction(() => ({ icon: 'refresh', label: 'Look again', run: refresh, progress: looking ? (progress?.value ?? 0) : null, line: progress?.line ?? null }));

  // Answered here: gone from the list at once (the server keeps the answer).
  let gone = $state<string[]>([]);
  let asking = $state<string | null>(null);
  // The filter (the pill's funnel, the owner 2026-10-24): a source left out leaves out what only it suggested; the
  // favourite artists' albums and those of artists the library has, when asked. Then the pill's search (as every
  // page's): what a card says, every word of it somewhere.
  const favourites = $derived(new Set(view?.favourite_artists ?? []));
  const passes = (a: DiscoverAlbum) =>
    !(a.sources?.length && a.sources.every((s) => switchOn('discover', `no-source:${s}`))) &&
    !(switchOn('discover', 'no-favourite-artists') && favourites.has(a.key)) &&
    !(switchOn('discover', 'no-library-artists') && !a.new_artist) &&
    // Unplayable: no page on Bandcamp to hear it on (the owner, 2026-10-25).
    !(switchOn('discover', 'no-unplayable') && !a.bandcamp) &&
    // Monitored: wanted in Lidarr already, or a request waiting or approved (the owner, 2026-10-26).
    !(switchOn('discover', 'no-monitored') && (!!a.request || askedFor(a.key)));
  /** Asked for already: monitored in Lidarr, or a request waiting or approved. */
  const askedFor = (key: string) => {
    const r = view?.requests?.[key];
    return !!r && (r.status === 'pending' || r.status === 'approved');
  };
  const kept = $derived((view?.found?.albums ?? []).filter((a) => !gone.includes(a.key)));
  // What the filter and the search let through, then by mode (below).
  const albums = $derived(
    kept.filter((a) => passes(a) && matchesSearch(a.album, a.artist, a.kind, a.year?.toString(), ...(a.genres ?? []), ...a.why, ...(a.sources ?? []))),
  );
  let pageWidth = $state(0);
  // Its mode (the picker beside back, lib/DiscoverPicker; the owner, 2026-10-29): each suggestion in the one whose reason
  // fits it best, in that mode's order. New artists are the artists themselves, each with their records that fit best.
  const spotifyView = $derived(isSpotifyView(prefs.discoverMode) ? prefs.discoverMode : null);
  const mode = $derived<ReasonMode>(inSpotify() ? 'fresh' : (prefs.discoverMode as ReasonMode));
  const inMode = (a: DiscoverAlbum) => modeOf(a) === mode;
  const cards = $derived(mode === 'artists' ? [] : inModeOrder(mode, albums.filter(inMode)));
  /** New artists: the server's, each with their records (those that pass); from a server before it, the new artists'
   *  records by artist, the best fit first. */
  const artists = $derived.by(() => {
    if (mode !== 'artists') return [];
    const shown = new Map(albums.filter(inMode).map((a) => [a.key, a]));
    const given = view?.found?.artists;
    if (given)
      return given
        .map((x) => ({ name: x.name, why: x.why, records: x.records.flatMap((k) => (shown.has(k) ? [shown.get(k)!] : [])) }))
        .filter((x) => x.records.length);
    const by = new Map<string, DiscoverAlbum[]>();
    for (const a of inModeOrder('artists', [...shown.values()])) (by.get(a.artist) ?? by.set(a.artist, []).get(a.artist)!).push(a);
    return [...by].map(([name, records]) => ({ name, why: [] as string[], records }));
  });
  // A Spotify list left on, and the account has none any more (disconnected, the export forgotten): Fresh.
  loadSpotify().then(() => {
    if (inSpotify() && spotify.state && !spotify.state.shown) prefs.discoverMode = 'fresh';
  });
  // More for you made as it comes near (lib/nearBlocks, as the Library's albums; the owner, 2026-10-25): blocks of six
  // whole rows of its grid (cards of 340 px at least, 12 px apart).
  const blocks = new NearBlocks();
  $effect(() => () => blocks.dispose());
  const restCols = $derived(Math.max(1, Math.floor((pageWidth + 12) / 352)));
  const restBlocks = $derived.by(() => {
    const n = restCols * 6;
    const out: (typeof cards)[] = [];
    for (let i = 0; i < cards.length; i += n) out.push(cards.slice(i, i + n));
    return out;
  });
  // How many the filter lets through, of how many (the Filter sheet's count).
  $effect(() => {
    pageView.counted = view?.found ? { shown: kept.filter((a) => inMode(a) && passes(a)).length, total: kept.filter(inMode).length } : null;
    return () => (pageView.counted = null);
  });
  async function want(a: DiscoverAlbum) {
    asking = a.key;
    try {
      const { request } = await wantDiscover(a.key);
      if (view) view.requests = { ...view.requests, [a.key]: request };
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      toast.fail(e, 'Could not ask for it');
    } finally {
      asking = null;
    }
  }
  /** Where an asked-for album stands, on its card. */
  function asked(r: AlbumRequest): string {
    const a = r.album_state;
    if (a?.state === 'in_library') return 'In the library';
    if (r.status === 'pending') return 'Requested';
    if (r.status === 'declined') return r.reason ? `Declined: ${r.reason}` : 'Declined';
    if (r.status === 'cancelled') return '';
    // Not handed over: its mark a red "!", the message in its sheet (the owner, 2026-10-26: it broke the card's layout).
    if (r.fetched?.startsWith('Not handed over')) return '';
    if (a?.state === 'fetching') return `Being fetched · ${Math.round(a.done * 100)}%`;
    if (a?.state === 'in_incoming') return 'In Incoming';
    if (a?.state === 'partly_there') return `Partly there · ${a.tracks} of ${a.of}`;
    return 'Approved';
  }
  /** Asked for and not taken back: Want is done for it. */
  const open = (r: AlbumRequest | undefined) => !!r && (r.status === 'pending' || r.status === 'approved');

  // Play (decision 0071): lib/bandcampFound's, one place for every cover that plays an album on Bandcamp.
  const previewing = $derived(bandcampPlay.key);
  const preview = (a: DiscoverAlbum) => a.bandcamp && playOnBandcamp(a.key, a.bandcamp, a.album);
  // What went wrong with a card's request or its play (the owner, 2026-10-26): its round mark a red "!", the message
  // in a sheet with Try again, never the card's words.
  const problemOf = (a: DiscoverAlbum) => {
    const f = view?.requests?.[a.key]?.fetched;
    return f?.startsWith('Not handed over') ? f : null;
  };
  let problem = $state<{ title: string; text: string; retry: () => void } | null>(null);
  function onproblem(a: DiscoverAlbum, what: 'request' | 'play') {
    const text = what === 'play' ? bandcampFailed[a.key] : problemOf(a);
    if (!text) return;
    problem = what === 'play' ? { title: `${a.album} did not play`, text, retry: () => preview(a) } : { title: `${a.album} was not handed over`, text, retry: () => want(a) };
  }

  /** The card's facts in one line: what it is, its tracks, its length. */
  function facts(a: DiscoverAlbum): string {
    const parts: string[] = [];
    if (a.kind) parts.push(a.kind);
    if (a.track_count) parts.push(`${a.track_count} ${a.track_count === 1 ? 'track' : 'tracks'}`);
    if (a.length_secs) parts.push(hoursMinutes(a.length_secs));
    if (a.listeners) parts.push(listeners(a.listeners));
    return parts.join(' · ');
  }
  const listeners = (n: number) =>
    `${n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : n} listeners`;
  /** The card's pills: one per page the album has elsewhere (the owner, 2026-10-24); a search there is no pill. */
  const pages = (a: DiscoverAlbum) => a.links.filter((l) => !l.name.startsWith('Search '));

  // The menu of the card tapped.
  let opened = $state<DiscoverAlbum | null>(null);
  // The action first, then the menu closed: what it acts on is read from the open menu, which closing empties (the
  // owner's Play on Bandcamp did nothing, 2026-10-25: it read a menu already closed).
  const act = (what: () => unknown) => () => {
    what();
    opened = null;
  };
  // A tap: what plays on Bandcamp opens as an album in the panel, its tracks to hear (decision 0071, the owner
  // 2026-10-25); the rest, which Cratarr cannot show, its menu. A long-press is always the menu.
  const tap = (a: DiscoverAlbum) => (a.bandcamp ? viewOver(albumSubject(BANDCAMP, albumIdOf(a.bandcamp))) : (opened = a));
  // A link to another site asks first, Open or Copy link, as every such link does (lib/LinkSheet).
  let link = $state<{ url: string; name: string } | null>(null);

  /** Not interested in an artist new to the library (the owner, 2026-10-29): they and their records leave every mode
   *  for good; Undo in its toast brings them back. */
  async function noArtist(a: DiscoverAlbum) {
    const theirs = kept.filter((x) => x.artist === a.artist).map((x) => x.key);
    gone = [...gone, ...theirs];
    try {
      const { undo } = await notDiscoverArtist(a.artist);
      withUndo(`Not interested in ${a.artist}.`, [undo], {
        after: (undone) => {
          gone = undone ? gone.filter((k) => !theirs.includes(k)) : [...gone, ...theirs];
          if (undone) load();
        },
      });
    } catch (e) {
      gone = gone.filter((k) => !theirs.includes(k));
      toast.fail(e, 'Could not leave them out');
    }
  }
  /** Not interested (`no`: its artist held against it, twice and they are left out) or Hide (nothing said of it); Undo
   *  in its toast brings it back (the one undo, decision 0026). */
  async function no(a: DiscoverAlbum, hide = false) {
    gone = [...gone, a.key];
    try {
      const { undo } = await (hide ? hideDiscover(a.key) : notDiscover(a.key));
      withUndo(hide ? 'Hidden.' : 'Not interested.', [undo], {
        after: (undone) => {
          gone = undone ? gone.filter((k) => k !== a.key) : [...gone, a.key];
          if (undone) load();
        },
      });
    } catch (e) {
      gone = gone.filter((k) => k !== a.key);
      toast.fail(e, 'Could not leave it out');
    }
  }
</script>

{#snippet how()}
  {#if progress}
    {#if progress.value !== null}<Meter label={progress.line} value={progress.value} />{:else}<p class="note">{progress.line}</p>{/if}
  {/if}
{/snippet}

{#snippet card(a: DiscoverAlbum)}
  {@const r = view?.requests?.[a.key]}
  <li class="card">
    <!-- A tap anywhere on the card opens its menu (the owner, 2026-10-24: every action in a menu), but on its
         pills, which are its links (each through the Open/Copy sheet, as every link to another site). -->
    <button class="open" aria-label={`${a.album} by ${a.artist}`} onclick={() => tap(a)} use:longpress={() => (opened = a)}></button>
    <!-- Cratarr's own cover: a picture that does not load is its stand-in, and the suggestion stays. -->
    <span class="cover"><Cover src={a.cover} /><CoverStatus mark={a.request ?? markOf(r)} problem={problemOf(a)} onproblem={() => onproblem(a, 'request')} />{#if a.bandcamp}<CoverPlay bandcamp label={a.album} busy={previewing === a.key} problem={bandcampFailed[a.key] ?? null} onplay={() => preview(a)} onproblem={() => onproblem(a, 'play')} />{/if}</span>
    <span class="text">
        <b>{a.album}</b>
        <span class="by">{a.artist}{a.year ? ` · ${a.year}` : ''}{#if a.new_artist}<span class="new">New to you</span>{/if}</span>
        {#if facts(a)}<small class="facts">{facts(a)}</small>{/if}
        {#if a.genres?.length}<span class="tags">{#each a.genres as g (g)}<span class="tag">{g}</span>{/each}</span>{/if}
        {#each a.why as w (w)}<small class="why">{w}</small>{/each}
        {#if pages(a).length}
          <span class="tags">{#each pages(a) as l (l.url)}<a class="tag link" href={l.url} target="_blank" rel="noopener noreferrer">{l.name}</a>{/each}</span>
        {/if}
        {#if r && asked(r)}
          <small class="asked" class:bad={r.status === 'declined' || r.fetched?.startsWith('Not handed over')}>{asked(r)}</small>
        {/if}
    </span>
  </li>
{/snippet}

<main>
  <div class="measure" bind:clientWidth={pageWidth}></div>
  {#if spotifyView}
    <SpotifyLibrary view={spotifyView} />
  {:else if error && !view}
    <Notice text={error} />
  {:else if !view}
    <!-- Waiting for the list: a look running says how far it is under the loader too, never only in the pill (the
         owner, 2026-10-25). -->
    <PageLoading>{@render how()}</PageLoading>
  {:else}
    {#if !connected}
      <p class="note">ListenBrainz or Last.fm would add albums like what you play: <a href="#/settings?view=settings%3Aconnections">connect one</a>.</p>
    {/if}
    {#if looking && !albums.length}
      <!-- The first look: the page's loader where it always is, how far it is under it (ui-conventions, Progress). -->
      <PageLoading>{@render how()}</PageLoading>
    {:else}
      {#each view.found?.notes ?? [] as note (note)}<p class="note">{note}</p>{/each}
    {/if}
    {#if !albums.length}
      <!-- Nothing found yet: the shared loading sign, not words of its own (the owner, 2026-10-26). -->
      {#if !looking}{#if kept.length || view.found}<p class="note">{kept.length ? 'None of them match.' : 'Nothing new to suggest right now.'}</p>{:else}<PageLoading />{/if}{/if}
    {:else}
      {#if me.admin && !view.lidarr}
        <p class="note">Want needs Lidarr: <a href="#/settings?view=settings%3Aconnections%2Flidarr">connect it</a>.</p>
      {/if}
      {#if mode === 'artists' ? !artists.length : !cards.length}<p class="note">None of them match.</p>{/if}
      {#each artists as x (x.name)}
        <SectionHead title={x.name} />
        {#each x.why as w (w)}<p class="why-artist">{w}</p>{/each}
        <ul class="grid">
          {#each x.records as a (a.key)}{@render card(a)}{/each}
        </ul>
      {/each}
      {#each restBlocks as block, bi (`${restCols}:${bi}`)}
      {@const key = `${restCols}:${bi}`}
      {@const made = blocks.made(key, bi === 0)}
      <div {@attach blocks.watch(key)} style:height={made ? null : `${blocks.heights[key] ?? Math.ceil(block.length / restCols) * 162}px`}>
      {#if made}
      <ul class="grid">
        {#each block as a (a.key)}
          {@render card(a)}
        {/each}
      </ul>
      {/if}
      </div>
      {/each}
    {/if}
  {/if}
</main>

{#if opened}
  {@const a = opened}
  {@const r = view?.requests?.[a.key]}
  <MenuSheet label={a.album} title={a.album} images={a.cover ? [a.cover] : []} chips={[a.artist, ...(a.year ? [String(a.year)] : [])]} copy={`${a.artist} – ${a.album}`} onclose={() => (opened = null)}>
    <!-- Groups as every menu's (lib/MenuCards); Discover's own (Want, Hide, Not interested) one of them, as in the
         player's menu of a Bandcamp preview (the owner, 2026-10-25). -->
    <MenuCards>
      <MenuGroup>
        {#if a.bandcamp}
          <MenuEntry icon="play" label="Play on Bandcamp" busy={previewing === a.key} onclick={act(() => preview(a))} />
        {/if}
        <!-- The library's artist their page, one new to it their page from Discover (lib/discoverArtist). -->
        <MenuEntry icon="user" label="Go to {a.artist}" onclick={act(() => goToArtist(a.new_artist ? DISCOVER : 'cratarr', a.artist))} />
      </MenuGroup>
      <MenuGroup label="Discover">
        {#if r && open(r)}
          <MenuEntry icon="inbox" label={r.status === 'pending' ? 'Requested' : 'Approved'} onclick={act(() => (location.hash = '#/requests'))} />
        {:else}
          <MenuEntry icon="circlePlus" label="Want" disabled={!canWant || asking === a.key} busy={asking === a.key} onclick={act(() => want(a))} />
        {/if}
        <MenuEntry icon="eyeOff" label="Hide" onclick={act(() => no(a, true))} />
        <MenuEntry icon="thumbsDown" label="Not interested" onclick={act(() => no(a))} />
        {#if a.new_artist}<MenuEntry icon="ban" label="Not interested in {a.artist}" onclick={act(() => noArtist(a))} />{/if}
      </MenuGroup>
      <MenuGroup>
        {#each a.links as l (l.url)}<MenuEntry icon="externalLink" label={l.name} onclick={act(() => (link = l))} />{/each}
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
{#if problem}{@const x = problem}<ProblemSheet title={x.title} text={x.text} retry={x.retry} onclose={() => (problem = null)} />{/if}
{#if link}{@const l = link}<LinkSheet url={l.url} name={l.name} onclose={() => (link = null)} />{/if}

<style>
  .why-artist {
    margin: -6px 0 10px;
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* The page's width, for the masonry. */
  .measure {
    height: 0;
  }

  /* The page answers to its own width (beside the player or not): a container, as Crates. */
  main {
    --bleed: clamp(12px, 2vw, 32px);
    position: relative;
    padding: 14px var(--bleed) 48px;
    container-type: inline-size;
  }

  .note {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 14px;
  }

  .note a {
    color: var(--accent-ink);
  }

  /* One card across a phone, more as the page widens (as Crates). */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* A card: a button under all of it opens its menu; its pills (links) lie over that button. */
  .card {
    position: relative;
    display: flex;
    gap: 14px;
    padding: 12px;
    border-radius: 14px;
    background: var(--surface);
    /* Only the cards on screen are drawn, as a playlist's rows (the owner, 2026-10-25: More for you is 250 long). */
    content-visibility: auto;
    contain-intrinsic-size: auto 150px;
  }

  .card:has(.open:hover),
  .card:has(.open:focus-visible) {
    background: color-mix(in srgb, var(--text) 6%, var(--surface));
  }

  .open {
    position: absolute;
    inset: 0;
    padding: 0;
    border: 0;
    border-radius: inherit;
    background: none;
    cursor: pointer;
  }

  /* What the card says lets a tap through to the button under it; its links and the cover's play button take their
     own (the owner, 2026-10-26: Play opened the album instead). */
  .cover,
  .text {
    position: relative;
    pointer-events: none;
  }

  .text a,
  .cover :global(.cover-play) {
    pointer-events: auto;
  }

  .cover {
    position: relative;
    flex: none;
    width: 112px;
    height: 112px;
    border-radius: 8px;
    overflow: hidden;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .cover :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  b {
    overflow-wrap: anywhere;
    font-size: 15px;
  }

  .by {
    color: var(--muted);
    font-size: 14px;
  }

  .new {
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-ink);
    font-size: 12px;
  }

  .facts,
  .why {
    color: var(--muted);
    font-size: 13px;
  }

  /* Where an asked-for album stands. */
  .asked {
    color: var(--accent-ink);
    font-size: 13px;
  }

  .asked.bad {
    color: var(--danger-ink);
  }

  /* Its genres, and where it comes from: each a small label. */
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tags:empty {
    display: none;
  }

  .tag {
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent-ink);
    font-size: 11px;
  }

  /* A page elsewhere: a pill to tap. */
  .tag.link {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
    text-decoration: none;
  }
</style>
