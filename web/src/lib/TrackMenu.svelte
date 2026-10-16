<script lang="ts" module>
  /** A track the menu is for, when it is not the playing one (a queue row, a playlist row; decision 0023). */
  export type MenuTrack = { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null };
  /** Where a queue row's track is: `upcoming` (what comes next, 0 = the next) or `previous` (this device's played ones). */
  export type QueuePlace = { list: 'upcoming' | 'previous'; at: number };
</script>

<script lang="ts">
  // A track's menu (decisions 0019, 0021, 0023): the playing track's (the menu button in the player, a right-click on
  // its cover) or any other's (a long-press on a row). A sheet as Finamp's track menu: the track on the card (with the
  // heart), Play next and Add to queue as the large actions, then: Add to playlist (the playlists as toggle cards, a tap
  // adds or removes), the favourite, and for the playing track a new playlist made of what plays and what comes next.
  // From a queue row (`queue`) it is about that place in the queue: Play now and Play next (moving it there on this
  // device; again, as a copy, on the server's player, which takes no reorder) as the large actions, and Remove from
  // queue last (this device, as the row's swipe).
  import {
    coverUrl,
    albumCoverUrl,
    playlistFromQueue,
    queueTrack,
    setTrackFavorite,
    setTrackMember,
    trackFile,
    trackState,
    type TrackFile,
    type TrackState,
  } from './api';
  import MarkLoader from './MarkLoader.svelte';
  import { pageView } from './pageView.svelte';
  import { player, refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { shownCover, favorite, pending, playing, toggleFavorite } from './remote.svelte';
  import { enqueue, jump as localJump, local, moveUpcoming, removeUpcoming } from './localPlayer.svelte';
  import { playUpcoming } from './remote.svelte';
  import { isLocal, setPlayingMember, shownPlaylists } from './output.svelte';
  import { setFavorite } from './favourites.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';
  import { followCreate, followDelete } from './follow.svelte';
  import { albumSubject, keepViewed, viewPlaylist } from './panel.svelte';
  import { download, inApp } from './app.svelte';

  let {
    onclose,
    track = null,
    start = 'menu',
    queue: place = null,
    onplaylists = null,
  }: {
    onclose: () => void;
    track?: MenuTrack | null;
    start?: 'menu' | 'lists';
    queue?: QueuePlace | null;
    /** Opened from a component over the whole page (the full player): its playlists' step offers the Playlists page. */
    onplaylists?: (() => void) | null;
  } = $props();

  const np = $derived(playing());
  /** The menu is for what plays (no track given, or the given one plays). */
  const forPlaying = $derived(!track || (!!np && np.source === track.source && np.id === track.id));
  const t = $derived<MenuTrack | null>(forPlaying ? np : track);

  // Another track's playlists and heart, asked for when its menu opens.
  let other = $state<TrackState | null>(null);
  $effect(() => {
    if (forPlaying || !track) return;
    const { source, id } = track;
    trackState(source, id).then(
      (v) => (other = v),
      (e) => toast.fail(e, "This track's playlists couldn't be loaded"),
    );
  });

  const src = $derived(
    forPlaying ? shownCover() : track ? (track.album_id ? albumCoverUrl(track.source, track.album_id) : coverUrl(track)) : null,
  );
  const addable = $derived(forPlaying ? !!np?.addable : !!other?.addable);
  /** The track's album, for View album: as the track came, else as the server says. */
  const albumId = $derived(t?.album_id ?? (forPlaying ? null : (other?.album_id ?? null)));
  function viewAlbum() {
    if (!t || !albumId) return;
    const subject = albumSubject(t.source, albumId);
    onclose();
    // From the full-screen player: it closes, or it would cover the album.
    player.full = false;
    viewPlaylist(subject);
  }
  const heartOn = $derived(forPlaying ? favorite() : !!other?.favorite);
  const heartKnown = $derived(forPlaying ? np?.favorite !== null : other !== null && other.favorite !== null);
  const busy = $derived(forPlaying ? pending() : !other);
  // Playlists holding the track first, then the rest by name.
  const playlists = $derived(
    [...(forPlaying ? shownPlaylists() : (other?.playlists ?? []))].sort(
      (a, b) => Number(b.member) - Number(a.member) || a.name.localeCompare(b.name),
    ),
  );

  // The step it opens on; later steps are the sheet's own.
  // svelte-ignore state_referenced_locally
  let step = $state<'menu' | 'lists' | 'new' | 'repeats' | 'details'>(start);
  // The queue a new playlist is made of; one that plays a track twice asks: Keep repeats, or each track once
  // (decision 0025, amended 2026-10-16).
  const queueIds = $derived(
    isLocal() ? local.queue.slice(Math.max(0, local.index)).map((x) => x.id) : np ? [np.id, ...np.upcoming.map((u) => u.id)] : [],
  );
  const queueRepeats = $derived(new Set(queueIds).size < queueIds.length);

  // Go to artist: the Library page with that artist's albums (the search says the name). A "feat." credit is left out:
  // the albums are under the main artist.
  const artistName = $derived(t?.artist?.split(/\s+(?:feat\.?|ft\.|featuring)\s+/i)[0].trim() || null);
  function goToArtist() {
    if (!artistName) return;
    pageView.artist = artistName;
    pageView.next = artistName;
    pageView.query = artistName;
    onclose();
    if (player.full) player.full = false;
    location.hash = keepViewed('#/library');
  }

  // Web search: the track's artist and title, in a new tab (the app opens it outside).
  function webSearch() {
    if (!t) return;
    const words = [t.artist, t.name].filter(Boolean).join(' ');
    window.open(`https://duckduckgo.com/?q=${encodeURIComponent(words)}`, '_blank', 'noopener');
    onclose();
  }

  // Details: the file as it is, read when the step opens.
  let details = $state<TrackFile | null>(null);
  let detailsFailed = $state(false);
  function showDetails() {
    if (!t) return;
    step = 'details';
    details = null;
    detailsFailed = false;
    trackFile(t.source, t.id).then(
      (d) => (details = d),
      () => (detailsFailed = true),
    );
  }
  const plays = $derived(forPlaying ? (np?.info?.play_count ?? null) : (other?.info?.play_count ?? null));
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
  const day = (s: number) => new Date(s * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  function facts(d: TrackFile): [string, string][] {
    const channels = d.channels === 1 ? 'mono' : d.channels === 2 ? 'stereo' : d.channels ? `${d.channels} channels` : null;
    const audio = [
      d.format,
      d.bit_depth ? `${d.bit_depth}-bit` : null,
      d.sample_rate_hz ? `${+(d.sample_rate_hz / 1000).toFixed(1)} kHz` : null,
      d.bitrate_kbps ? `${d.bitrate_kbps} kbps` : null,
      channels,
    ].filter(Boolean);
    const out: [string, string][] = [];
    if (audio.length) out.push(['Audio', audio.join(' · ')]);
    if (d.duration_ms) out.push(['Length', clock(d.duration_ms / 1000)]);
    if (d.size) out.push(['Size', `${(d.size / 1e6).toFixed(1)} MB`]);
    if (plays !== null) out.push(['Plays', String(plays)]);
    if (d.added) out.push(['Added', day(d.added)]);
    out.push(['File', d.folder ? `${d.folder}/${d.path}` : d.path]);
    for (const tag of d.tags) out.push([tag.name, tag.value]);
    return out;
  }
  let working = $state(false);
  let nameField = $state('');

  async function heart() {
    if (forPlaying) return toggleFavorite();
    if (!track || !other || other.favorite === null) return;
    try {
      other = await setTrackFavorite(track.source, track.id, !other.favorite);
      // The rows' hearts show it too.
      if (other.favorite !== null) setFavorite(track.source, track.id, other.favorite);
      refresh();
    } catch (e) {
      toast.fail(e, "The favourite didn't change");
    }
  }

  // A queue row's actions.
  const here = $derived(isLocal());
  // Each action reads what it is for before closing: the opener drops the menu's track and place on close.
  async function playNow() {
    const at = place;
    if (!at) return;
    onclose();
    try {
      if (at.list === 'previous') localJump(at.at);
      else await playUpcoming(at.at);
    } catch (e) {
      toast.fail(e, "The player didn't go to that track");
    }
  }
  async function playNextFromQueue() {
    const at = place;
    if (!at) return;
    // On this device an upcoming track moves to be next; otherwise (a played one, the server's player) a copy goes next.
    if (here && at.list === 'upcoming') {
      onclose();
      moveUpcoming(at.at, 0);
      return;
    }
    await queue('next');
  }
  function removeFromQueue() {
    const at = place;
    if (!at || !here || at.list !== 'upcoming') return;
    onclose();
    removeUpcoming(at.at);
  }
  const movesHere = $derived(!!place && here && place.list === 'upcoming');

  async function queue(how: 'next' | 'last') {
    const tr = t;
    if (!tr) return;
    working = true;
    onclose();
    try {
      if (isLocal()) enqueue([{ source: tr.source, id: tr.id, name: tr.name, artist: tr.artist, album: tr.album, album_id: tr.album_id }], how);
      else await queueTrack(tr.id, how, tr.source);
      toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      setTimeout(refresh, 700);
    } catch (e) {
      toast.fail(e, how === 'next' ? "It couldn't be put next" : "It couldn't be added to the queue");
    } finally {
      working = false;
    }
  }

  async function toggle(id: string, member: boolean) {
    if (!t || busy) return;
    try {
      if (forPlaying) await setPlayingMember(id, !member);
      else if (track) {
        other = await setTrackMember(track.source, track.id, id, !member);
        refresh();
      }
    } catch (e) {
      toast.fail(e, member ? "It couldn't be taken out" : "It couldn't be added");
    }
  }

  async function makePlaylist(repeats = false) {
    const name = nameField.trim();
    if (!name || working) return;
    working = true;
    try {
      const ids = isLocal() ? local.queue.slice(Math.max(0, local.index)).map((t) => t.id) : undefined;
      const made = await playlistFromQueue(name, t?.source, ids, repeats);
      const left = made.left_out ? `, ${made.left_out} not in the library left out` : '';
      withUndo(`Made ${made.playlist}: ${made.added} ${made.added === 1 ? 'track' : 'tracks'}${left}.`, [made.undo], {
        after: (undone) => (undone ? followDelete(made.playlist) : followCreate(made.playlist)),
      });
      onclose();
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
    } finally {
      working = false;
    }
  }
</script>

{#if t}
  <MenuSheet
    label={t.name}
    {onclose}
    images={src ? [src] : []}
    title={t.name}
    chips={[t.artist, t.album].filter((c): c is string => !!c)}
    heart={heartKnown ? { on: heartOn, disabled: busy, toggle: heart } : null}
    actions={step === 'lists' && onplaylists
      ? [
          {
            icon: 'playlists',
            label: 'Playlists',
            onclick: () => {
              onclose();
              onplaylists();
            },
          },
        ]
      : step !== 'menu'
        ? []
        : place && !forPlaying
        ? [
            { icon: 'play', label: 'Play now', onclick: playNow, disabled: working },
            { icon: 'playNext', label: movesHere ? 'Play next' : 'Play again next', onclick: playNextFromQueue, disabled: working },
          ]
        : [
            { icon: 'playNext', label: 'Play next', onclick: () => queue('next'), disabled: working },
            { icon: 'playlists', label: 'Add to queue', onclick: () => queue('last'), disabled: working },
          ]}
    onback={step === 'menu' ? null : () => (step = step === 'repeats' ? 'new' : 'menu')}
    {step}
  >
    {#if step === 'menu'}
      <div role="menu">
        {#if addable}
          <MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />
        {/if}
        {#if heartKnown}
          <MenuEntry icon="heart" label={heartOn ? 'Remove from favourites' : 'Add to favourites'} disabled={busy} onclick={heart} />
        {/if}
        {#if albumId}
          <MenuEntry icon="squareArrowOutUpRight" label="View album" onclick={viewAlbum} />
        {/if}
        {#if artistName}
          <MenuEntry icon="user" label="Go to {artistName}" onclick={goToArtist} />
        {/if}
        <MenuEntry icon="info" label="Details" onclick={showDetails} />
        <MenuEntry icon="search" label="Web search" onclick={webSearch} />
        {#if inApp && t}
          {@const one = t}
          <MenuEntry
            icon="arrowDownToLine"
            label="Download"
            onclick={() => {
              download([one]);
              onclose();
            }}
          />
        {/if}
        {#if forPlaying}
          <MenuEntry
            icon="plus"
            label="New playlist from the queue"
            onclick={() => {
              nameField = '';
              step = 'new';
            }}
          />
        {/if}
        {#if movesHere && !forPlaying}
          <MenuEntry icon="trash" label="Remove from queue" danger onclick={removeFromQueue} />
        {/if}
      </div>
    {:else if step === 'lists'}
      <div class="heading">Add to / remove from playlists</div>
      <div role="group" aria-label="Playlists">
        {#each playlists as p (p.id)}
          <MenuEntry label={p.name} checked={p.member} disabled={busy} onclick={() => toggle(p.id, p.member)} />
        {/each}
      </div>
    {:else if step === 'details'}
      {#if details}
        <dl class="facts">
          {#each facts(details) as [name, value], i (i)}
            <dt>{name}</dt>
            <dd>{value}</dd>
          {/each}
        </dl>
      {:else if detailsFailed}
        <p class="failed">The file can't be read.</p>
      {:else}
        <div class="loading"><MarkLoader size={40} /></div>
      {/if}
    {:else if step === 'repeats'}
      <div class="heading">The queue plays some tracks more than once</div>
      <div role="group" aria-label="Repeated tracks">
        <MenuEntry icon="listAdd" label="Keep repeats" disabled={working} onclick={() => makePlaylist(true)} />
        <MenuEntry icon="check" label="Each track once" disabled={working} onclick={() => makePlaylist(false)} />
      </div>
    {:else}
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          if (queueRepeats) step = 'repeats';
          else makePlaylist();
        }}
      >
        <div class="heading">New playlist from the queue</div>
        <input type="text" aria-label="Name of the new playlist" placeholder="Name" maxlength="100" bind:value={nameField} />
        <button type="submit" class="primary" disabled={working || !nameField.trim()} aria-busy={working}>Make playlist</button>
      </form>
    {/if}
  </MenuSheet>
{/if}

<style>
  /* Details: name and value per line, the names muted; a long value (a path, a list) wraps. */
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
    padding: 0 16px 12px;
    font-size: 0.9rem;
  }

  .facts dt {
    color: var(--muted);
  }

  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .failed {
    padding: 0 16px 12px;
    color: var(--danger, var(--muted));
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }

  .heading {
    padding: 0 16px 12px;
    font-size: 18px;
    text-align: center;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form .heading {
    padding: 0 0 4px;
  }

  .form input {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .primary {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--art-accent, var(--accent));
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
