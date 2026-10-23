<script lang="ts">
  // Home (the owner, 2026-10-23: the leftmost tab): the quick look. New in your library (the newest albums, a swipe
  // row; All opens the Library by date added), Sounds like your week (the week's crate: what sounds like the last seven
  // days' plays, none of them), Jump back in (the albums and playlists played last, and the queues left last), and
  // Needs you (claude-632's,
  // lib/HomeNeeds). A tile with nothing to show is left out.
  import { onMount } from 'svelte';
  import { coverUrl, crateRows, getCrate, playlistStats, type PlaylistRow } from './lib/api';
  import AlbumCard from './lib/AlbumCard.svelte';
  import { addedOn, albumUses, library, loadAlbumUses, readLibrary } from './lib/albums.svelte';
  import { albumCoverUrl } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import HomeNeeds from './lib/HomeNeeds.svelte';
  import { LIKE, viewGenre, viewLike } from './lib/crates.svelte';
  import { DEVICE } from './lib/deviceMusic.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { hiddenLibraries } from './lib/listFilter.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { albumSubject, viewOver, viewPlaylist } from './lib/panel.svelte';
  import { playCollection } from './lib/play.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { loadQueues, queues, queuesVersion, switchQueue } from './lib/queueHistory.svelte';
  import SectionHead from './lib/SectionHead.svelte';
  import { toast } from './lib/toast.svelte';

  const NEWEST = 12;
  const WEEK = `${LIKE}week`;

  onMount(() => {
    if (!library.view) void readLibrary(false);
  });
  const sources = $derived((library.view?.sources ?? []).filter((s) => s.id !== DEVICE));

  // New in your library: every used library's newest albums.
  const newest = $derived(
    sources
      .flatMap((s) => s.albums.filter((a) => a.added !== null).map((a) => ({ source: s.id, a })))
      .sort((x, y) => y.a.added! - x.a.added!)
      .slice(0, NEWEST),
  );
  function allNewest() {
    prefs.libraryOrder = 'added';
    prefs.libraryDown = true;
    location.hash = '#/library';
  }

  // Sounds like your week: its first rows (for the covers), its size, its genres, and how many tracks were played.
  let week = $state<{ name: string; source: string; total: number; rows: PlaylistRow[]; genres: string[]; played: number } | null>(null);
  onMount(() => {
    Promise.all([crateRows(WEEK, 0, 40, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() }), getCrate(WEEK)]).then(
      ([r, d]) => {
        const like = d.conditions.find((c) => c.value.kind === 'like')?.value;
        week = { name: d.name, source: r.source, total: r.total, rows: r.rows, genres: r.genres.slice(0, 4), played: like?.kind === 'like' ? like.of.length : 0 };
      },
      () => (week = null),
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

  // Jump back in: the albums played last (each server's plays) and the playlists played last.
  $effect(() => {
    for (const s of sources) if (!albumUses[s.id]) void loadAlbumUses(s.id);
  });
  let lastPlayed = $state<Record<string, number>>({});
  onMount(() => {
    playlistStats().then(
      (st) => (lastPlayed = Object.fromEntries(Object.entries(st).flatMap(([id, s]) => (s.last_played ? [[id, s.last_played]] : [])))),
      () => {},
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
  function open(b: Back) {
    if (b.kind === 'album') viewOver(albumSubject(b.source, b.id));
    else if (b.kind === 'playlist') viewPlaylist(b.id);
    // Taken up where it was left (playing if it played).
    else void switchQueue(b.id);
  }
</script>

<div class="home">
  {#if newest.length}
    <section aria-label="New in your library">
      <SectionHead title="New in your library" onmore={allNewest} />
      <ul class="shelf">
        {#each newest as n (n.source + n.a.id)}
          <li><AlbumCard source={n.source} album={n.a} lines={[n.a.artist ?? '', `added ${addedOn(n.a.added!)}`]} /></li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if week && week.total > 0}
    <section aria-label="Sounds like your week">
      <SectionHead title="Sounds like your week" />
      <div class="week">
        {#if weekCovers.length >= 4}
          <button class="mosaic" onclick={() => viewLike('week')} aria-label="Open {week.name}">
            {#each weekCovers as src (src)}<span><Cover {src} /></span>{/each}
          </button>
        {/if}
        <div class="about">
          <h3>{week.total.toLocaleString()} tracks you didn't play this week</h3>
          {#if week.played}<p>from the {week.played.toLocaleString()} tracks you played in the last 7 days</p>{/if}
          {#if week.genres.length}<p class="genres">{#each week.genres as g (g)}<button onclick={() => viewGenre(g)}>{g}</button>{/each}</p>{/if}
          <div class="acts">
            <button class="main" disabled={starting} onclick={() => playWeek('play')}><Icon name="play" size={18} />Play</button>
            <button disabled={starting} onclick={() => playWeek('shuffle')}><Icon name="shuffle" size={18} />Shuffle</button>
            <button onclick={() => viewLike('week')}>Open</button>
          </div>
        </div>
      </div>
    </section>
  {/if}

  {#if back.length}
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

  <!-- Needs you (lib/HomeNeeds): an administrator's, shown only when something waits. -->
  <HomeNeeds />
</div>

<style>
  .home {
    display: flex;
    flex-direction: column;
    gap: 28px;
    padding: 16px 0 32px;
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

  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }

  .acts button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .acts button.main {
    background: var(--accent);
    color: var(--on-accent, #000);
  }

  .acts button:disabled {
    opacity: 0.6;
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
