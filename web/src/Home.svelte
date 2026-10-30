<script lang="ts">
  // Home (the owner, 2026-10-23: the leftmost tab): the quick look. The moment (named after the weather and the time
  // of day: a few albums near it and a radio inside it, the server's), New for you (the newest albums not heard yet: less
  // than 5% of their length, a swipe row; All opens the Library by date added), Sounds like your week (the week's crate: what sounds like the last seven
  // days' plays, none of them), Jump back in (the albums and playlists played last, and the queues left last), Blast
  // from the past (the favourite tracks unplayed longest) and Grave digging (the albums most like the taste and furthest
  // from the listening), and System status (lib/HomeStatus). A tile with nothing to show is left out.
  import { onMount, untrack, type Snippet } from 'svelte';
  import {
    albumView,
    coverUrl,
    getDigging,
    getFavoriteAlbums,
    getFavoriteTracks,
    getHomeBack,
    getHomeNew,
    getHomeWeek,
    getMoment,
    type Digging,
    type HomeBack,
    type HomeWeek,
    type ListedAlbum,
    type Moment,
    type PlaylistRow,
  } from './lib/api';
  import AlbumMenu from './lib/AlbumMenu.svelte';
  import { longpress } from './lib/longpress';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import QueuesSheet from './lib/QueuesSheet.svelte';
  import TrackMenu, { type MenuTrack } from './lib/TrackMenu.svelte';
  import AlbumCard from './lib/AlbumCard.svelte';
  import { addedOn, library, readLibrary } from './lib/albums.svelte';
  import { albumCoverUrl, crateRows, getCrate } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import CoverFlow from './lib/CoverFlow.svelte';
  import ArtistsToExplore from './lib/ArtistsToExplore.svelte';
  import HomeStatus from './lib/HomeStatus.svelte';
  import { CRATE, LIKE, viewGenre, viewLike } from './lib/crates.svelte';
  import { device, DEVICE, localMode } from './lib/deviceMusic.svelte';
  import { startRadio } from './lib/localPlayer.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { hiddenLibraries } from './lib/listFilter.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { albumSubject, viewOver, viewPlaylist } from './lib/panel.svelte';
  import { playCollection } from './lib/play.svelte';
  import { CRATE_PART, FLOW_PARTS, prefs, type HomePart } from './lib/prefs.svelte';
  import { backdropUrl } from './lib/backdrop.svelte';
  import { me } from './lib/me.svelte';
  import greet from 'greet-by-time';
  import { queuesPlayer, queuesVersion, switchQueue } from './lib/queueHistory.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { holdUntil } from './lib/holdPlace.svelte';
  import { toast } from './lib/toast.svelte';
  import { albumAway, away, here, rowsAway } from './lib/away.svelte';
  import { askDevicePlays, backHere, devicePlays, LOCAL_PARTS, newestHere, playHereAlbums } from './lib/localHome.svelte';

  const WEEK = `${LIKE}week`;
  const NEWEST_HERE = 'Newest on this device';
  /** Local mode (lib/localHome): the device's own Home, nothing asked of Cratarr. */
  const local = localMode();
  if (local) askDevicePlays();

  onMount(() => {
    if (!library.view) void readLibrary(false);
  });
  const sources = $derived((library.view?.sources ?? []).filter((s) => s.id !== DEVICE));

  // New for you (the owner, 2026-10-23 and 2026-10-30): the server's (one rule with its crate): Cratarr's own library's
  // newest albums, those heard 5% or more of and those without a cover left out, at most 3 of an album artist's.
  let serverNewest = $state<{ source: string; a: ListedAlbum }[]>([]);
  let serverNewestIn = $state(false);
  const newest = $derived(local ? newestHere().map((a) => ({ source: DEVICE, a })) : serverNewest);
  const newestIn = $derived(local ? device.tracks !== null : serverNewestIn);
  onMount(() => {
    if (local) return;
    getHomeNew().then(
      (r) => ((serverNewest = r.albums.map(({ source, ...a }) => ({ source, a }))), (serverNewestIn = true)),
      () => (serverNewestIn = true),
    );
  });
  /** The section on top's Play (the owner, 2026-10-24): all of it, from the album in front (those before it stay before
   *  it in the queue). Blast from the past: its tracks from the front album's; the others: their albums one after the
   *  other in their order, on the front one's server. */
  async function playFeatured() {
    const front = frontAlbum;
    if (!front) return;
    if (lead === 'past') return playPast('play', dug?.past.find((t) => t.album_id === front.id)?.id);
    const albums = heroAlbums.filter((a) => a.source === front.source);
    if (front.source === DEVICE) return playHereAlbums(albums.map((a) => a.id), front.id, leadTitle);
    starting = true;
    try {
      const views = await Promise.all(albums.map((a) => albumView(a.source, a.id).catch(() => null)));
      const ids: string[] = [];
      let from: string | undefined;
      views.forEach((v, i) => {
        const own = v ? v.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : [])) : [];
        if (albums[i].id === front.id) from = own[0];
        ids.push(...own);
      });
      if (ids.length) await playCollection({ kind: 'tracks', source: front.source, ids }, 'play', leadTitle, from);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }

  // Sounds like your week: the server's (app::home): its size, its genres, how many tracks were played, and its first
  // albums with a cover for the mosaic.
  let week = $state<HomeWeek | null>(null);
  let weekIn = $state(false);
  onMount(() => {
    if (local) return void (weekIn = true);
    getHomeWeek(prefs.source || undefined, hiddenLibraries()).then(
      (r) => ((week = r.week), (weekIn = true)),
      () => ((week = null), (weekIn = true)),
    );
  });
  /** Its first four albums, as a seeded crate's mosaic (lib/PlaylistView). */
  const weekCovers = $derived((week?.covers ?? []).map((c) => coverUrl({ source: week!.source, id: c.track, album_id: c.album_id })));
  let starting = $state(false);
  async function playWeek(how: 'play' | 'shuffle') {
    if (!week) return;
    starting = true;
    try {
      await playCollection({ kind: 'crate', id: WEEK }, how, week.name, undefined, { without: hiddenLibraries() });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }

  // The moment (the owner, 2026-10-23: its own style, not a shelf like those below): its albums as a Flow on a canvas
  // painted for the sky, the front one named; its radio plays only what is near it, the moment read again on each
  // refill (it follows the weather).
  let moment = $state<Moment | null>(null);
  let momentIn = $state(false);
  onMount(() => {
    if (local) return void (momentIn = true);
    getMoment(prefs.source || undefined, hiddenLibraries()).then(
      (m) => ((moment = m), (momentIn = true)),
      () => ((moment = null), (momentIn = true)),
    );
  });
  const momentAlbums = $derived(
    (moment?.albums ?? []).flatMap((id) => sources.find((s) => s.id === 'cratarr')?.albums.find((x) => x.id === id) ?? []),
  );
  let momentFront = $state(0);
  /** Room under the moment's covers for their shadow (CoverFlow's foot). */
  const SHADOW = 40;
  /** The sky's photo (public/skies, hand-picked from Unsplash): sunny and hot are day skies, clear a night's; the others
   *  by day or after sunset. Made whole against the page where it is set: a var() resolves a relative url() against the
 *  stylesheet that uses it, assets/ in a build (the tablet asked for assets/skies/…, 404). */
  const skyPhoto = $derived.by(() => {
    // A picture of the user's own, chosen on this device (Settings › Appearance), in place of the weather's where asked
    // (Also on Home; the owner, 2026-10-26: it is every flow's backdrop first).
    if (prefs.flowPictureOnHome && backdropUrl()) return backdropUrl();
    // Without the weather, the plain sky of the hour.
    if (!moment) return `skies/plain-${new Date().getHours() >= 7 && new Date().getHours() < 19 ? 'day' : 'night'}.jpg`;
    const one: Record<string, string> = { sunny: 'sunny-day', hot: 'hot-day', clear: 'clear-night' };
    return `skies/${one[moment.sky] ?? `${moment.sky}-${moment.dark ? 'night' : 'day'}`}.jpg`;
  });
  async function momentRadio() {
    if (!moment) return;
    navigator.vibrate?.(8);
    starting = true;
    try {
      await startRadio([], moment.name, `${LIKE}now`);
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    } finally {
      starting = false;
    }
  }

  // Jump back in: the server's (app::home): the albums and playlists played last and this player's queues left, a queue
  // that is an album's tracks shown as that album; asked again when the queues change.
  let serverBack = $state<HomeBack[]>([]);
  let serverBackIn = $state(false);
  const back = $derived(local ? backHere() : serverBack);
  const backIn = $derived(local ? device.tracks !== null && devicePlays.known : serverBackIn);
  $effect(() => {
    if (local) return;
    void queuesVersion();
    getHomeBack(queuesPlayer()).then(
      (r) => ((serverBack = r.items), (serverBackIn = true)),
      () => (serverBackIn = true),
    );
  });
  /** A queue's icon by where it came from, as the queue switcher has it (lib/QueuesSheet). */
  const FROM: Record<string, IconName> = { playlist: 'playlists', album: 'albums', favourites: 'heart', library: 'library', tracks: 'music', player: 'speaker' };
  // Blast from the past and Grave digging (the server's), of the libraries switched on.
  let dug = $state<Digging | null>(null);
  let dugIn = $state(false);
  onMount(() => {
    if (local) return void (dugIn = true);
    getDigging(prefs.source || undefined, hiddenLibraries()).then(
      (d) => ((dug = d), (dugIn = true)),
      () => ((dug = null), (dugIn = true)),
    );
  });
  // Blast from the past: each album once (the owner, 2026-10-23), by its track unplayed longest; Play plays them all.
  const pastAlbums = $derived(dug ? dug.past.filter((t, i, all) => all.findIndex((x) => x.album_id === t.album_id) === i) : []);
  const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  /** How long ago (epoch seconds), in the largest unit that fits. */
  function ago(t: number) {
    const days = Math.max(0, Math.round((Date.now() / 1000 - t) / 86_400));
    if (days < 14) return relative.format(-days, 'day');
    if (days < 60) return relative.format(-Math.round(days / 7), 'week');
    if (days < 730) return relative.format(-Math.round(days / 30), 'month');
    return relative.format(-Math.round(days / 365), 'year');
  }
  /** Grave digging's albums, as the Library lists them. */
  const graves = $derived(
    (dug?.graves ?? []).flatMap((g) => {
      const a = sources.find((s) => s.id === 'cratarr')?.albums.find((x) => x.id === g.id);
      return a ? [{ a, last: g.last_played }] : [];
    }),
  );
  /** Plays Blast from the past (`from`: from that track). */
  async function playPast(how: 'play' | 'shuffle', from?: string) {
    if (!dug?.past.length) return;
    starting = true;
    try {
      await playCollection({ kind: 'tracks', source: 'cratarr', ids: dug.past.map((t) => t.id) }, how, 'Blast from the past', from);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }

  // Favourite albums and Favourite tracks (the owner, 2026-10-24: the standard ones, off until switched on): Cratarr's
  // own, asked for only when switched on.
  let favAlbumIds = $state<string[] | null>(null);
  let favRows = $state<PlaylistRow[] | null>(null);
  $effect(() => {
    if (local || !order.includes('favAlbums') || favAlbumIds) return;
    getFavoriteAlbums('cratarr').then(
      (ids) => (favAlbumIds = ids),
      () => (favAlbumIds = []),
    );
  });
  $effect(() => {
    if (local || !order.includes('favTracks') || favRows) return;
    getFavoriteTracks().then(
      (r) => (favRows = r.rows.filter((x) => x.id && !x.missing)),
      () => (favRows = []),
    );
  });
  const favAlbums = $derived.by(() => {
    const own = sources.find((s) => s.id === 'cratarr')?.albums ?? [];
    return (favAlbumIds ?? []).flatMap((id) => own.find((a) => a.id === id) ?? []);
  });
  /** Plays the favourite tracks (`from`: from that one). */
  async function playFavourites(from?: string) {
    if (!favRows?.length) return;
    starting = true;
    try {
      await playCollection({ kind: 'tracks', source: 'cratarr', ids: favRows.map((t) => t.id!) }, 'play', 'Favourite tracks', from);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }

  // Out of reach (lib/away; the owner, 2026-10-28): Home as last seen, what is not on this device dimmed, and what
  // cannot play without Cratarr (a radio, a section none of whose tracks are here) without its Play.
  const pastAway = $derived(!!dug && rowsAway('cratarr', dug.past.map((t) => t.id)));
  const favAway = $derived(!!favRows && rowsAway('cratarr', favRows.map((t) => t.id)));

  // Progressive loading (the owner, 2026-10-23: "the first component should show first and the second, instead of
  // scattered"): a section shows once its answer is in and every one above it has shown (or come back empty), top to
  // bottom; one slow answer holds those under it no longer than PATIENCE, then each shows as it comes.
  let exploreIn = $state(false);
  let statusIn = $state(false);
  const PATIENCE = 4000;
  let patient = $state(true);
  onMount(() => {
    const t = setTimeout(() => (patient = false), PATIENCE);
    return () => clearTimeout(t);
  });
  type Part = HomePart;
  /** Its sections switched on, in the user's order (Settings › Home); one's own crates while Your crates on Home is on. */
  const order = $derived(prefs.homeParts.filter((p) => (local ? LOCAL_PARTS.includes(p) : !p.startsWith(CRATE_PART) || prefs.homeCrates)));

  // One's own crates on Home (0.9.3; the owner, 2026-10-25): each a shelf of its albums, as New for you's, its › the
  // crate; read once a visit, its first albums in the crate's order (`null`: nothing to show).
  const CRATE_ALBUMS = 20;
  type CrateShelf = { name: string; source: string; albums: { id: string; title: string; artist: string | null; year: number | null }[] };
  let crateShelves = $state<Record<string, CrateShelf | null>>({});
  const asking = new Set<string>();
  $effect(() => {
    const ids = order.filter((p) => p.startsWith(CRATE_PART)).map((p) => p.slice(CRATE_PART.length));
    for (const id of untrack(() => ids.filter((x) => !(x in crateShelves) && !asking.has(x)))) {
      asking.add(id);
      Promise.all([crateRows(id, 0, 300, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() }), getCrate(id)]).then(
        ([page, def]) => {
          const albums: CrateShelf['albums'] = [];
          for (const r of page.rows) {
            if (!r.album_id || albums.some((a) => a.id === r.album_id)) continue;
            albums.push({ id: r.album_id, title: r.album ?? '', artist: r.album_artist ?? r.artist ?? null, year: r.year ?? null });
            if (albums.length === CRATE_ALBUMS) break;
          }
          crateShelves[id] = { name: def.name, source: page.source, albums };
        },
        () => (crateShelves[id] = null),
      );
    }
  });
  /** Whether a section has what it needs to show (a crate's: its shelf read). */
  const settledOf = (part: Part) => (part.startsWith(CRATE_PART) ? part.slice(CRATE_PART.length) in crateShelves : settled[part]);
  const settled = $derived<Record<Part, boolean>>({
    moment: momentIn && (!moment || !!library.view),
    newest: newestIn,
    week: weekIn,
    back: backIn,
    past: dugIn,
    graves: dugIn && !!library.view,
    explore: exploreIn,
    status: statusIn,
    favAlbums: favAlbumIds !== null && !!library.view,
    favTracks: favRows !== null,
  });
  // The header's picture first (the owner, 2026-10-26: "The first thing to show should be the header image. After that,
  // the rest can start to enter"): with a flow leading Home, its sky shows as soon as it is known which (the moment's
  // weather, or one's own picture) and that picture is in, then its words and albums, then the sections after it; two
  // and a half seconds at most, then whatever there is.
  let skyIn = $state(false);
  /** The sections a first screen shows: the lead and the one after it. */
  const FIRST_SCREEN = 2;
  $effect(() => {
    if (skyIn) return;
    if (!heroLead) return void (skyIn = true);
    const t = setTimeout(() => (skyIn = true), 2500);
    if (!((prefs.flowPictureOnHome && backdropUrl()) || momentIn) || !skyPhoto) return () => clearTimeout(t);
    let gone = false;
    const img = new Image();
    img.onload = img.onerror = () => !gone && (skyIn = true);
    img.src = new URL(skyPhoto, document.baseURI).href;
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  const turn = (part: Part) =>
    settledOf(part) && (part === lead || !heroLead || skyIn) && (!patient || order.slice(0, order.indexOf(part)).every((x) => settledOf(x)));
  /** Nothing shown yet and something still to come: the page's loader, not a blank page. */
  const loading = $derived(!order.some(turn) && !order.every((x) => settledOf(x)));
  // One commit (lib/Hold; the owner, 2026-10-26: "Hold until its first screen is ready"): the page before stays until
  // the header's picture and the first sections are in; those further down may follow.
  holdUntil(
    () => 0,
    () => (!heroLead || skyIn) && order.slice(0, FIRST_SCREEN).every((x) => settledOf(x)),
  );

  /** The European days a greeting wishes (make-a-wish's dates without the American ones, the owner 2026-10-24), by
   *  `month-day`. */
  const DAYS: Record<string, string> = {
    '1-1': 'Happy New Year',
    '2-14': "Happy Valentine's Day",
    '3-8': "Happy International Women's Day",
    '4-22': 'Happy Earth Day',
    '5-1': 'Happy Labour Day',
    '6-21': 'Happy World Music Day',
    '8-19': 'Happy World Photography Day',
    '9-21': 'Happy International Day of Peace',
    '10-31': 'Happy Halloween',
    '12-25': 'Merry Christmas',
    '12-31': "Happy New Year's Eve",
  };
  /** One pick per visit: the greeting stays while the page shows. */
  const pick = Math.random();
  /** The personal greeting (Settings › Home): a European day's wish on its day, else one of greet-by-time's greetings
   *  for the time of day (the night is the evening's), with their name when it is known. */
  const greetWords = $derived.by(() => {
    const now = new Date();
    const h = now.getHours();
    const day = DAYS[`${now.getMonth() + 1}-${now.getDate()}`];
    const list = h >= 5 && h < 12 ? greet.morningGreetings : h >= 12 && h < 18 ? greet.dayGreetings : greet.eveningGreetings;
    return day ?? list[Math.floor(pick * list.length)];
  });
  /** The account's name as greeted: its display name where the directory has one (0.10; the owner, 2026-10-30: "we
   *  should simply use the display name"), else the account's name as it is. */
  const greetName = $derived(me.display_name || me.name);
  const greeting = $derived(greetName ? `${greetWords}, ${greetName}!` : `${greetWords}!`);
  /** A word worth repeating: not "a", "the", "for". */
  const wordsOf = (s: string) => new Set(s.toLowerCase().match(/[a-z]+/g)?.filter((w) => w.length > 3) ?? []);
  /** The moment's own name under the greeting, unless the two share a word ("Good evening" over "Music for a Rainy
   *  Evening"): then "Best for now" (the owner, 2026-10-24). */
  const momentUnder = $derived.by(() => {
    const name = moment?.name ?? '';
    const said = wordsOf(greetWords);
    return [...wordsOf(name)].some((w) => said.has(w)) ? 'Best for now' : name;
  });

  // The first section leads Home as the flow on top, over the sky (the owner, 2026-10-24): only those made of albums
  // can lead (FLOW_PARTS; Settings › Home keeps one of them first). Its albums, its title.
  const lead = $derived(order[0]);
  type FlowAlbum = { source: string; id: string; title: string; artist: string | null };
  const flowOf = (source: string, a: { id: string; title: string; artist: string | null }): FlowAlbum => ({ source, id: a.id, title: a.title, artist: a.artist });
  const heroAlbums = $derived.by((): FlowAlbum[] => {
    const own = sources.find((s) => s.id === 'cratarr')?.albums ?? [];
    switch (lead) {
      case 'moment':
        return momentAlbums.map((a) => flowOf('cratarr', a));
      case 'newest':
        return newest.map((n) => flowOf(n.source, n.a));
      case 'past':
        return pastAlbums.flatMap((t) => own.filter((a) => a.id === t.album_id).map((a) => flowOf('cratarr', a)));
      case 'graves':
        return graves.map((g) => flowOf('cratarr', g.a));
      case 'favAlbums':
        return favAlbums.map((a) => flowOf('cratarr', a));
      default:
        return [];
    }
  });
  const heroLead = $derived(FLOW_PARTS.includes(lead));
  /** The header's words and albums, after its picture. */
  const heroReady = $derived(skyIn && turn(lead) && heroAlbums.length > 0);

  const TITLES: Partial<Record<Part, string>> = { newest: local ? NEWEST_HERE : 'New for you', past: 'Blast from the past', graves: 'Grave digging', favAlbums: 'Favourite albums' };
  const heroAway = $derived(away() && heroAlbums.every((a) => albumAway(a.source, a.id)));
  const leadTitle = $derived(lead === 'moment' ? (moment?.name ?? '') : (TITLES[lead] ?? ''));
  /** The album in front of the flow, named under it. */
  const frontAlbum = $derived(heroAlbums[Math.min(momentFront, heroAlbums.length - 1)]);

  /** Each section, drawn where the user's order puts it. */
  const PARTS: Record<Part, Snippet> = {
    moment: momentPart,
    newest: newestPart,
    week: weekPart,
    back: backPart,
    past: pastPart,
    graves: gravesPart,
    explore: explorePart,
    status: statusPart,
    favAlbums: favAlbumsPart,
    favTracks: favTracksPart,
  };

  // A long-press or a right-click opens an item's own menu, as everywhere (decision 0058; the owner, 2026-10-24: on
  // Home it did nothing): an album's, a track's, a playlist's, the queues'.
  let albumMenu = $state<{ source: string; id: string; title: string; artist: string | null } | null>(null);
  let trackMenu = $state<MenuTrack | null>(null);
  let playlistMenu = $state<string | null>(null);
  let queuesOpen = $state(false);
  function menuFor(b: HomeBack) {
    if (b.kind === 'album' && b.source === DEVICE) return;
    if (b.kind === 'album') albumMenu = { source: b.source, id: b.id, title: b.title, artist: b.by };
    else if (b.kind === 'playlist') playlistMenu = b.id;
    else queuesOpen = true;
  }
  const playlistOfMenu = $derived(playlistMenu ? (shownPlaylists().find((p) => p.id === playlistMenu) ?? null) : null);

  function open(b: HomeBack) {
    if (b.kind === 'album') viewOver(albumSubject(b.source, b.id));
    else if (b.kind === 'playlist') viewPlaylist(b.id);
    // Taken up where it was left (playing if it played).
    else void switchQueue(b.id);
  }
</script>

{#if albumMenu}
  {@const m = albumMenu}
  <AlbumMenu source={m.source} album={{ id: m.id, title: m.title, artist: m.artist }} onclose={() => (albumMenu = null)} />
{/if}
{#if trackMenu}<TrackMenu track={trackMenu} onclose={() => (trackMenu = null)} />{/if}
{#if playlistOfMenu}<PlaylistMenu playlists={[playlistOfMenu]} onclose={() => (playlistMenu = null)} />{/if}
{#if queuesOpen}<QueuesSheet onclose={() => (queuesOpen = false)} />{/if}

<div class="home">
  {#if loading}<PageLoading />{/if}
  <!-- Its sections in the user's order, those switched on (Settings › Home; the owner, 2026-10-24). -->
  {#each order as part, i (part)}
    {#if part.startsWith(CRATE_PART)}{@render cratePart(part.slice(CRATE_PART.length))}{:else}{@render (i === 0 && FLOW_PARTS.includes(part) ? heroPart : PARTS[part])()}{/if}
  {/each}
</div>

{#snippet heroPart()}
  <!-- Its sky at once, its picture as soon as it is in; its words and albums after it (heroReady). Not under the page's
       loader: its sky's top showed below it as a hard edge (the owner, 2026-10-26); it comes whole, its fade with it. -->
  {#if !loading && (!settledOf(lead) || heroAlbums.length)}
    <section
      class="moment top"
      class:waiting={!heroReady}
      data-sky={moment?.sky ?? 'plain'}
      class:dark={moment?.dark ?? false}
      style:--photo={skyIn && skyPhoto ? `url("${new URL(skyPhoto, document.baseURI)}")` : 'none'}
      aria-label={leadTitle}
    >
      {#if prefs.homeGreeting}
        <!-- The personal greeting (Settings › Home): the time of day and their name, then what plays here. -->
        <h2 class="greeting">{greeting}</h2>
        <p class="for-now">{lead === 'moment' ? momentUnder : leadTitle}</p>
      {:else}
        <h2>{leadTitle}</h2>
      {/if}
      <div class="sky-flow">
        {#if heroReady}<CoverFlow
          enter
          covers={heroAlbums.map((a) => albumCoverUrl(a.source, a.id))}
          top={0}
          foot={SHADOW}
          onopen={(i) => viewOver(albumSubject(heroAlbums[i].source, heroAlbums[i].id))}
          onfront={(i) => (momentFront = i)}
          onmenu={(i) => heroAlbums[i].source !== DEVICE && (albumMenu = { ...heroAlbums[i] })}
        />{/if}
      </div>
      <div class="caption">
        {#if frontAlbum}
          <button
            class="name"
            onclick={() => viewOver(albumSubject(frontAlbum.source, frontAlbum.id))}
            use:longpress={() => frontAlbum.source !== DEVICE && (albumMenu = { ...frontAlbum })}
          >
            <span class="title">{frontAlbum.title}</span>
            <span class="by">{frontAlbum.artist ?? ''}</span>
          </button>
        {/if}
        {#if lead === 'moment'}
          {#if !away()}<button class="radio" disabled={starting} onclick={momentRadio}><Icon name="radioTower" size={18} />Radio</button>{/if}
        {:else if lead === 'past' ? !pastAway : !heroAway}
          <button class="radio" disabled={starting} onclick={playFeatured}><Icon name="play" size={18} />Play</button>
        {/if}
      </div>
    </section>
  {/if}
{/snippet}

{#snippet momentPart()}
  <!-- The moment further down (not leading): a shelf as New for you's, its covers a little bigger, its › opening its
       crate (the owner, 2026-10-24; its Radio is on top only). -->
  {#if turn('moment') && moment && momentAlbums.length}
    <section aria-label={moment.name}>
      <SectionHead title={moment.name} onmore={() => viewLike('now')} />
      <ul class="shelf big">
        {#each momentAlbums as a (a.id)}
          <li><AlbumCard source="cratarr" album={a} lines={[a.artist ?? '', a.year ? String(a.year) : '']} /></li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet cratePart(id: string)}
  <!-- One's own crate (0.9.3): its albums as New for you's shelf, › its crate. -->
  {@const s = crateShelves[id]}
  {#if s && s.albums.length && turn(`${CRATE_PART}${id}`)}
    <section aria-label={s.name}>
      <SectionHead title={s.name} onmore={() => viewPlaylist(CRATE + id)} />
      <ul class="shelf">
        {#each s.albums as a (a.id)}
          <li><AlbumCard source={s.source} album={a} lines={[a.artist ?? '', a.year ? String(a.year) : '']} /></li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet newestPart()}
  {#if turn('newest') && newest.length}
    <section aria-label="New for you">
      <SectionHead title={local ? NEWEST_HERE : 'New for you'} onmore={() => (local ? (location.hash = '#/local/albums') : viewPlaylist(CRATE + 'home:new'))} />
      <ul class="shelf">
        {#each newest as n (n.source + n.a.id)}
          <li><AlbumCard source={n.source} album={n.a} lines={[n.a.artist ?? '', `added ${addedOn(n.a.added!)}`]} /></li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet weekPart()}
  {#if turn('week') && week && week.total > 0}
    <section aria-label="Sounds like your week">
      <SectionHead title="Sounds like your week" onplay={() => playWeek('play')} busy={starting} />
      <div class="week">
        {#if weekCovers.length >= 4}
          <button class="mosaic" onclick={() => viewLike('week')} aria-label="Open {week.name}">
            {#each weekCovers as src (src)}<span><Cover {src} /></span>{/each}
          </button>
        {/if}
        <div class="about">
          <h3>{week.total.toLocaleString()} tracks you didn't play this week</h3>
          {#if week.played}<p>like the {week.played.toLocaleString()} tracks you played in the last 7 days</p>{/if}
          {#if week.genres.length}<p class="genres">{#each week.genres as g (g)}<button onclick={() => viewGenre(g)}>{g}</button>{/each}</p>{/if}
        </div>
      </div>
    </section>
  {/if}
{/snippet}

{#snippet backPart()}
  {#if turn('back') && back.length}
    <section aria-label="Jump back in">
      <SectionHead title="Jump back in" />
      <ul class="back">
        {#each back as b (b.kind + b.id)}
          <li>
            <button class:gone={b.kind === 'album' && albumAway(b.source, b.id)} onclick={() => open(b)} use:longpress={() => menuFor(b)}>
              {#if b.kind === 'album'}
                <span class="cover"><Cover src={albumCoverUrl(b.source, b.id)} /></span>
              {:else}
                <span class="cover icon"><Icon name={b.kind === 'queue' ? (FROM[b.from ?? ''] ?? 'music') : 'playlists'} size={26} /></span>
              {/if}
              <span class="text">
                <span class="title">{b.title}</span>
                <span class="sub">{b.kind === 'album' ? ['Album', b.by].filter(Boolean).join(' · ') : b.kind === 'queue' ? `Queue · ${b.tracks} ${b.tracks === 1 ? 'track' : 'tracks'}` : 'Playlist'}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet pastPart()}
  {#if turn('past') && dug?.past.length}
    <section aria-label="Blast from the past">
      <SectionHead title="Blast from the past" onplay={pastAway ? undefined : () => playPast('play')} busy={starting} />
      <ul class="back">
        {#each pastAlbums as t (t.id)}
          <li>
            <!-- A tap opens its album, as every card does (the owner, 2026-10-25: it played at once, with no play to be
                 seen); the section's Play plays them, the long-press is the track's menu. -->
            <button
              class:gone={away() && !here('cratarr', t.id)}
              onclick={() => viewOver(albumSubject('cratarr', t.album_id))}
              use:longpress={() => (trackMenu = { source: 'cratarr', id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id })}
            >
              <span class="cover"><Cover src={albumCoverUrl('cratarr', t.album_id)} /></span>
              <span class="text">
                <span class="title">{t.name}</span>
                <span class="sub">{t.artist} · {ago(t.last_played)}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet gravesPart()}
  {#if turn('graves') && graves.length}
    <section aria-label="Grave digging">
      <SectionHead title="Grave digging" onmore={() => viewPlaylist(CRATE + 'home:graves')} />
      <ul class="shelf">
        {#each graves as g (g.a.id)}
          <li><AlbumCard source="cratarr" album={g.a} lines={[g.a.artist ?? '', g.last === null ? 'never played' : `played ${ago(g.last)}`]} /></li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet explorePart()}
  <!-- Artists to explore (lib/ArtistsToExplore; the owner, 2026-10-23). -->
  <div class="turn" class:held={!turn('explore')}><ArtistsToExplore onsettled={() => (exploreIn = true)} /></div>
{/snippet}

{#snippet statusPart()}
  <!-- System status (lib/HomeStatus): an administrator's, shown only when something waits. -->
  <div class="turn" class:held={!turn('status')}><HomeStatus onsettled={() => (statusIn = true)} /></div>
{/snippet}

{#snippet favAlbumsPart()}
  {#if turn('favAlbums') && favAlbums.length}
    <section aria-label="Favourite albums">
      <SectionHead title="Favourite albums" />
      <ul class="shelf">
        {#each favAlbums as a (a.id)}
          <li><AlbumCard source="cratarr" album={a} lines={[a.artist ?? '', a.year ? String(a.year) : '']} /></li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet favTracksPart()}
  {#if turn('favTracks') && favRows?.length}
    <section aria-label="Favourite tracks">
      <SectionHead title="Favourite tracks" onplay={favAway ? undefined : () => playFavourites()} busy={starting} />
      <ul class="back">
        {#each favRows.slice(0, 12) as t (t.id)}
          <li>
            <!-- As Blast from the past's: a tap opens its album, the section's Play plays them, a long-press its menu. -->
            <button
              class:gone={away() && !here('cratarr', t.id)}
              onclick={() => t.album_id && viewOver(albumSubject('cratarr', t.album_id))}
              use:longpress={() => (trackMenu = { source: 'cratarr', id: t.id!, name: t.name ?? t.file, artist: t.artist, album: t.album, album_id: t.album_id ?? null })}
            >
              <span class="cover"><Cover src={coverUrl({ source: 'cratarr', id: t.id!, album_id: t.album_id })} /></span>
              <span class="text">
                <span class="title">{t.name ?? t.file}</span>
                <span class="sub">{[t.artist, t.album].filter(Boolean).join(' · ')}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

<style>
  /* A part that loads itself (it asks on mounting): there from the start, shown in its turn. */
  .turn {
    display: contents;
  }

  .turn.held {
    display: none;
  }

  .home {
    display: flex;
    flex-direction: column;
    gap: 28px;
    padding: 16px 0 32px;
  }

  /* The moment: a canvas the width of the page from its top, the sky's photo under a dark veil, cropped towards its top
     (the owner: air, not landscape), its colours while the photo loads or without one, fading into the page at its foot;
     its words light on it in either theme. */
  .moment {
    --sky-top: #34465f;
    --sky-foot: #1a2230;
    --veil: 0.5;
    --top: var(--sky-top);
    --foot: var(--sky-foot);
    position: relative;
    margin-top: -16px;
    padding: 20px 0 36px;
    color: #eef0f4;
    background:
      linear-gradient(to bottom, rgb(0 0 0 / calc(var(--veil) - 0.1)), rgb(0 0 0 / var(--veil)) calc(100% - 60px), var(--bg)),
      var(--photo) center 20% / cover no-repeat,
      linear-gradient(to bottom, var(--top), var(--foot));
  }

  /* Under the page's row (back and the rest, floating over the page's top: App's .under-row), the sky goes on up behind
     it to the page's top, its heading where it was (the owner, 2026-10-23); only when it is Home's first section. */
  /* Without the row's room (Home opened first, beside the sidebar): its first content under the top-right glass, not
     behind it (the owner, 2026-10-28); the sky still from the page's top. */
  :global(.page:not(.under-row)) .home {
    padding-top: calc(var(--glass-room) + 16px);
  }

  :global(.page:not(.under-row)) .moment.top {
    margin-top: calc(-1 * (var(--glass-room) + 16px));
    padding-top: calc(var(--glass-room) + 20px);
  }

  :global(.page.under-row) .moment.top {
    margin-top: -76px;
    padding-top: 80px;
  }

  .moment[data-sky='sunny'] { --sky-top: #9a6424; --sky-foot: #3d2a16; }
  .moment[data-sky='clear'] { --sky-top: #24305e; --sky-foot: #10152b; }
  .moment[data-sky='fewclouds'] { --sky-top: #3f5c80; --sky-foot: #1d2a3b; }
  .moment[data-sky='cloudy'] { --sky-top: #46505e; --sky-foot: #1f242c; }
  .moment[data-sky='foggy'] { --sky-top: #565b63; --sky-foot: #26292e; }
  .moment[data-sky='rainy'] { --sky-top: #3a4d68; --sky-foot: #1b2331; }
  .moment[data-sky='stormy'] { --sky-top: #43375f; --sky-foot: #181426; }
  .moment[data-sky='snowy'] { --sky-top: #5b6e86; --sky-foot: #25303d; }
  .moment[data-sky='windy'] { --sky-top: #33545c; --sky-foot: #172528; }
  .moment[data-sky='hot'] { --sky-top: #8a3a1a; --sky-foot: #33160c; }
  .moment[data-sky='plain'] { --sky-top: #2e3644; --sky-foot: #171b22; }

  /* After sunset the same sky, darker (a clear sky is a night's already; a hot night's photo is the day's, veiled more). */
  .moment.dark:not([data-sky='clear']) {
    --top: color-mix(in srgb, var(--sky-top) 70%, #05070d);
    --foot: color-mix(in srgb, var(--sky-foot) 70%, #05070d);
  }

  .moment.dark[data-sky='hot'] {
    --veil: 0.65;
  }

  .moment h2 {
    margin: 0;
    padding: 0 16px;
    font-size: 23px;
    font-weight: 400;
  }

  /* The personal greeting a little larger than a section's title; the leading section's name under it as one (the
     owner, 2026-10-24). */
  .moment h2.greeting {
    font-size: 28px;
  }

  .moment .for-now {
    margin: 4px 0 0;
    padding: 0 16px;
    font-size: 23px;
  }

  /* Room under the covers for their shadow (it fades over 30 px): at the flow's very foot, its clip cut the shadow off
     in a hard line (the owner, 2026-10-23). The caption comes up as much. */
  /* Its words and caption wait for its picture (heroReady), then fade in. */
  .moment > :is(h2, .for-now, .caption) {
    transition: opacity 300ms ease;
  }

  .moment.waiting > :is(h2, .for-now, .caption) {
    opacity: 0;
    pointer-events: none;
  }

  .sky-flow {
    position: relative;
    height: calc(clamp(200px, 30vw, 290px) + 40px);
    margin-top: 16px;
  }

  .caption {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    position: relative;
    z-index: 1;
    max-width: 640px;
    margin: -24px auto 0;
    padding: 0 16px;
  }

  .caption .name {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  /* Each line a fixed height: a title in another script (Chinese, Japanese) falls back to a font with taller lines,
     and the page under it moved with every album that came to the front (the owner, 2026-10-24). */
  .caption .title,
  .caption .by {
    height: 1.4em;
    overflow: hidden;
    line-height: 1.4;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caption .title {
    font-weight: 500;
  }

  .caption .by {
    color: rgb(238 240 244 / 70%);
    font-size: 0.9rem;
  }

  /* The leading section's Radio (the moment) or Play, in its caption. */
  .radio {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    cursor: pointer;
  }

  .radio:disabled {
    opacity: 0.6;
  }

  .shelf {
    display: flex;
    gap: 12px;
    margin: 0;
    padding: 4px 16px 0;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-padding-inline: 16px;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
    list-style: none;
  }

  .shelf li {
    flex: none;
    width: 150px;
    scroll-snap-align: start;
  }

  .shelf.big li {
    width: 190px;
  }

  /* One line each on a shelf's cards: a shelf is as tall as its tallest card, and one long title anywhere in it, out of
     sight too, left a gap under all of them (the owner, 2026-10-23). */
  .shelf :global(.title),
  .shelf :global(.year) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .week {
    margin: 4px 16px 0;
    overflow: hidden;
    border-radius: 16px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .mosaic {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .mosaic span {
    display: block;
    aspect-ratio: 1;
    /* Square whatever the image's shape: without it a portrait cover stretched its box, and its card (the owner,
       2026-10-24). */
    overflow: hidden;
  }

  .about {
    padding: 14px 16px 16px;
  }

  h3 {
    margin: 0;
    font-size: 1.1rem;
  }

  .about p {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .about p.genres {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .genres button {
    padding: 2px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .back {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 8px;
    margin: 4px 16px 0;
    padding: 0;
    list-style: none;
  }

  .back button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 10px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  /* Out of reach and not on this device (lib/away), as an album's card. */
  .back button.gone {
    opacity: 0.45;
  }

  .back .cover {
    flex: none;
    display: block;
    width: 64px;
    height: 64px;
  }

  .back .cover.icon {
    display: grid;
    place-items: center;
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .back .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding-right: 10px;
  }

  .back .title,
  .back .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .back .sub {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
