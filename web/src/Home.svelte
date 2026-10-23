<script lang="ts">
  // Home (the owner, 2026-10-23: the leftmost tab): the quick look. The moment (named after the weather and the time
  // of day: a few albums near it and a radio inside it, the server's), New for you (the newest albums not heard yet: less
  // than 5% of their length, a swipe row; All opens the Library by date added), Sounds like your week (the week's crate: what sounds like the last seven
  // days' plays, none of them), Jump back in (the albums and playlists played last, and the queues left last), Blast
  // from the past (the favourite tracks unplayed longest) and Grave digging (the albums most like the taste and furthest
  // from the listening), and System status (claude-632's, lib/HomeStatus). A tile with nothing to show is left out.
  import { onMount } from 'svelte';
  import { coverUrl, crateRows, getCrate, getDigging, getMoment, heardAlbums, playlistStats, type Digging, type Moment, type PlaylistRow } from './lib/api';
  import AlbumCard from './lib/AlbumCard.svelte';
  import { addedOn, albumUses, albumUsesKnown, library, loadAlbumUses, readLibrary } from './lib/albums.svelte';
  import { albumCoverUrl } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import CoverFlow from './lib/CoverFlow.svelte';
  import ArtistsToExplore from './lib/ArtistsToExplore.svelte';
  import HomeStatus from './lib/HomeStatus.svelte';
  import { LIKE, viewGenre, viewLike } from './lib/crates.svelte';
  import { DEVICE } from './lib/deviceMusic.svelte';
  import { startRadio } from './lib/localPlayer.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { hiddenLibraries } from './lib/listFilter.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { albumSubject, viewOver, viewPlaylist } from './lib/panel.svelte';
  import { playCollection } from './lib/play.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { loadQueues, queues, queuesVersion, switchQueue } from './lib/queueHistory.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { toast } from './lib/toast.svelte';

  const NEWEST = 12;
  const WEEK = `${LIKE}week`;

  onMount(() => {
    if (!library.view) void readLibrary(false);
  });
  const sources = $derived((library.view?.sources ?? []).filter((s) => s.id !== DEVICE));

  // New for you (the owner, 2026-10-23): every used library's newest albums, those heard 5% or more of left out (asked
  // of the server for the newest few dozen).
  const CANDIDATES = 60;
  const candidates = $derived(
    sources
      .flatMap((s) => s.albums.filter((a) => a.added !== null).map((a) => ({ source: s.id, a })))
      .sort((x, y) => y.a.added! - x.a.added!)
      .slice(0, CANDIDATES),
  );
  let heard = $state<Set<string> | null>(null);
  let heardOf: string | null = null;
  // Asked again whenever the candidates change (the library read after Home showed): until the answer, none show
  // rather than albums already heard that then drop out (the owner, 2026-10-23).
  $effect(() => {
    const ids = candidates.filter((c) => c.source === 'cratarr').map((c) => c.a.id);
    // The same albums (the library read again): what was heard stands.
    const asked = ids.join(',');
    if (asked === heardOf) return;
    heardOf = asked;
    heard = null;
    if (!ids.length) {
      if (library.view) heard = new Set();
      return;
    }
    let gone = false;
    heardAlbums(ids).then(
      (r) => !gone && (heard = new Set(r.heard)),
      () => !gone && (heard = new Set()),
    );
    return () => (gone = true);
  });
  const newest = $derived(heard ? candidates.filter((c) => !(c.source === 'cratarr' && heard!.has(c.a.id))).slice(0, NEWEST) : []);
  function allNewest() {
    prefs.libraryOrder = 'added';
    prefs.libraryDown = true;
    location.hash = '#/library';
  }

  // Sounds like your week: its first rows (for the covers), its size, its genres, and how many tracks were played.
  let week = $state<{ name: string; source: string; total: number; rows: PlaylistRow[]; genres: string[]; played: number } | null>(null);
  let weekIn = $state(false);
  onMount(() => {
    Promise.all([crateRows(WEEK, 0, 40, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() }), getCrate(WEEK)]).then(
      ([r, d]) => {
        const like = d.conditions.find((c) => c.value.kind === 'like')?.value;
        week = { name: d.name, source: r.source, total: r.total, rows: r.rows, genres: r.genres.slice(0, 4), played: like?.kind === 'like' ? like.of.filter((_, i) => (like.weights?.[i] ?? 1) > 0).length : 0 };
        weekIn = true;
      },
      () => ((week = null), (weekIn = true)),
    );
  });
  /** Its first four albums, as a seeded crate's mosaic (lib/PlaylistView). */
  const weekCovers = $derived.by(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const r of week?.rows ?? []) {
      if (!r.id || r.missing) continue;
      const key = r.album_id ?? `${r.album ?? ''}\n${r.album_artist ?? r.artist ?? ''}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(coverUrl({ source: week!.source, id: r.id, album_id: r.album_id }));
      if (out.length === 4) break;
    }
    return out;
  });
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
   *  by day or after sunset. */
  const skyPhoto = $derived.by(() => {
    if (!moment) return null;
    const one: Record<string, string> = { sunny: 'sunny-day', hot: 'hot-day', clear: 'clear-night' };
    return `skies/${one[moment.sky] ?? `${moment.sky}-${moment.dark ? 'night' : 'day'}`}.jpg`;
  });
  const frontAlbum = $derived(momentAlbums[Math.min(momentFront, momentAlbums.length - 1)]);
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

  // Jump back in: the albums played last (each server's plays) and the playlists played last.
  $effect(() => {
    for (const s of sources) if (!albumUses[s.id]) void loadAlbumUses(s.id);
  });
  let lastPlayed = $state<Record<string, number>>({});
  let lastPlayedIn = $state(false);
  onMount(() => {
    playlistStats().then(
      (st) => ((lastPlayed = Object.fromEntries(Object.entries(st).flatMap(([id, s]) => (s.last_played ? [[id, s.last_played]] : [])))), (lastPlayedIn = true)),
      () => (lastPlayedIn = true),
    );
  });
  // The queues this device's player keeps (lib/queueHistory), read again when it says they changed.
  $effect(() => {
    void queuesVersion();
    void loadQueues();
  });
  /** A queue's icon by where it came from, as the queue switcher has it (lib/QueuesSheet). */
  const FROM: Record<string, IconName> = { playlist: 'playlists', album: 'albums', favourites: 'heart', library: 'library', tracks: 'music', player: 'speaker' };
  type Back =
    | { kind: 'album'; at: number; source: string; id: string; title: string; by: string | null }
    | { kind: 'playlist'; at: number; id: string; title: string }
    | { kind: 'queue'; at: number; id: string; title: string; tracks: number; from: string | null };
  /** Jump back in, by kind (the owner, 2026-10-23): how far back it looks (days) and how many it offers at most. */
  const BACK: Record<Back['kind'], { days: number; most: number }> = {
    album: { days: 14, most: 4 },
    playlist: { days: 30, most: 3 },
    queue: { days: 3, most: 2 },
  };
  /** All of them in one list, the latest first (the owner: purely chronological), each kind within its own cutoff and limit. */
  const back = $derived.by((): Back[] => {
    const now = Date.now() / 1000;
    const recent = (b: Back) => b.at > now - BACK[b.kind].days * 86_400;
    const latest = (list: Back[]) => list.filter(recent).sort((x, y) => y.at - x.at).slice(0, BACK[list[0]?.kind ?? 'album'].most);
    const albums = sources.flatMap((s) =>
      s.albums.flatMap((a): Back[] => {
        const at = albumUses[s.id]?.[a.id]?.played;
        return at ? [{ kind: 'album', at, source: s.id, id: a.id, title: a.title, by: a.artist }] : [];
      }),
    );
    const lists = shownPlaylists().flatMap((p): Back[] => (lastPlayed[p.id] ? [{ kind: 'playlist', at: lastPlayed[p.id], id: p.id, title: p.name }] : []));
    // The queues left; not the one that plays.
    const left = queues.list.filter((q) => !q.playing).map((q): Back => ({ kind: 'queue', at: q.updated, id: q.id, title: q.name, tracks: q.tracks, from: q.from }));
    return [...latest(albums), ...latest(lists), ...latest(left)].sort((x, y) => y.at - x.at);
  });
  // Blast from the past and Grave digging (the server's), of the libraries switched on.
  let dug = $state<Digging | null>(null);
  let dugIn = $state(false);
  onMount(() => {
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
  const ORDER = ['moment', 'newest', 'week', 'back', 'past', 'graves', 'explore', 'status'] as const;
  type Part = (typeof ORDER)[number];
  const settled = $derived<Record<Part, boolean>>({
    moment: momentIn && (!moment || !!library.view),
    newest: heard !== null,
    week: weekIn,
    back: lastPlayedIn && queues.known && !!library.view && sources.every((s) => albumUsesKnown(s.id)),
    past: dugIn,
    graves: dugIn && !!library.view,
    explore: exploreIn,
    status: statusIn,
  });
  const turn = (part: Part) => settled[part] && (!patient || ORDER.slice(0, ORDER.indexOf(part)).every((x) => settled[x]));
  /** Nothing shown yet and something still to come: the page's loader, not a blank page. */
  const loading = $derived(!ORDER.some(turn) && !ORDER.every((x) => settled[x]));

  function open(b: Back) {
    if (b.kind === 'album') viewOver(albumSubject(b.source, b.id));
    else if (b.kind === 'playlist') viewPlaylist(b.id);
    // Taken up where it was left (playing if it played).
    else void switchQueue(b.id);
  }
</script>

<div class="home">
  {#if loading}<PageLoading />{/if}
  {#if turn('moment') && moment && momentAlbums.length}
    <section class="moment" data-sky={moment.sky} class:dark={moment.dark} style:--photo={skyPhoto ? `url(${skyPhoto})` : 'none'} aria-label={moment.name}>
      <h2>{moment.name}</h2>
      <div class="sky-flow">
        <CoverFlow
          covers={momentAlbums.map((a) => albumCoverUrl('cratarr', a.id))}
          top={0}
          foot={SHADOW}
          onopen={(i) => viewOver(albumSubject('cratarr', momentAlbums[i].id))}
          onfront={(i) => (momentFront = i)}
        />
      </div>
      <div class="caption">
        {#if frontAlbum}
          <button class="name" onclick={() => viewOver(albumSubject('cratarr', frontAlbum.id))}>
            <span class="title">{frontAlbum.title}</span>
            <span class="by">{frontAlbum.artist ?? ''}</span>
          </button>
        {/if}
        <button class="radio" disabled={starting} onclick={momentRadio}><Icon name="radioTower" size={18} />Radio</button>
      </div>
    </section>
  {/if}

  {#if turn('newest') && newest.length}
    <section aria-label="New for you">
      <SectionHead title="New for you" onmore={allNewest} />
      <ul class="shelf">
        {#each newest as n (n.source + n.a.id)}
          <li><AlbumCard source={n.source} album={n.a} lines={[n.a.artist ?? '', `added ${addedOn(n.a.added!)}`]} /></li>
        {/each}
      </ul>
    </section>
  {/if}

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

  {#if turn('back') && back.length}
    <section aria-label="Jump back in">
      <SectionHead title="Jump back in" />
      <ul class="back">
        {#each back as b (b.kind + b.id)}
          <li>
            <button onclick={() => open(b)}>
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

  {#if turn('past') && dug?.past.length}
    <section aria-label="Blast from the past">
      <SectionHead title="Blast from the past" onplay={() => playPast('play')} busy={starting} />
      <ul class="back">
        {#each pastAlbums as t (t.id)}
          <li>
            <button disabled={starting} onclick={() => playPast('play', t.id)}>
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

  {#if turn('graves') && graves.length}
    <section aria-label="Grave digging">
      <SectionHead title="Grave digging" />
      <ul class="shelf">
        {#each graves as g (g.a.id)}
          <li><AlbumCard source="cratarr" album={g.a} lines={[g.a.artist ?? '', g.last === null ? 'never played' : `played ${ago(g.last)}`]} /></li>
        {/each}
      </ul>
    </section>
  {/if}

  <!-- Artists to explore (lib/ArtistsToExplore; the owner, 2026-10-23). -->
  <div class="turn" class:held={!turn('explore')}><ArtistsToExplore onsettled={() => (exploreIn = true)} /></div>

  <!-- System status (lib/HomeStatus): an administrator's, shown only when something waits. -->
  <div class="turn" class:held={!turn('status')}><HomeStatus onsettled={() => (statusIn = true)} /></div>
</div>

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
     it to the page's top, its heading where it was (the owner, 2026-10-23). */
  :global(.page.under-row) .moment {
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

  /* Room under the covers for their shadow (it fades over 30 px): at the flow's very foot, its clip cut the shadow off
     in a hard line (the owner, 2026-10-23). The caption comes up as much. */
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

  .caption .title,
  .caption .by {
    overflow: hidden;
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

  .caption .radio {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent, #000);
    font: inherit;
    cursor: pointer;
  }

  .caption .radio:disabled {
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
