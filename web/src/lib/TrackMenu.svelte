<script lang="ts" module>
  /** A track the menu is for, when it is not the playing one (a queue row, a playlist row; decision 0023). */
  /** `has_versions`: the library holds other recordings of its work (0.10); absent where the list does not say. */
  export type MenuTrack = { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null; has_versions?: boolean };
  /** Where a queue row's track is: `upcoming` (what comes next, 0 = the next) or `previous` (this device's played ones). */
  export type QueuePlace = { list: 'upcoming' | 'previous'; at: number };
</script>

<script lang="ts">
  // A track's menu (decisions 0019, 0021, 0023): the playing track's (the menu button in the player, a right-click on
  // its cover) or any other's (a long-press on a row). A sheet as Finamp's track menu: the track on the card (with the
  // heart), Play next, Add to queue and Start radio as the large actions, then in the album menu's order: Go to album,
  // Go to its artist and genres, the favourite, Add to playlist (the playlists as toggle cards, a tap adds or removes),
  // Download…, Share, Info, Web search, Adjust BPM (administrators: lib/TempoStep), in the table's order (lib/menuTable).
  // A Bandcamp preview queues as any track (0.10) and is never downloaded (decision 0071).
  // From a queue row (`queue`) it is about that place in the queue: Play now and Play next (moving it there on this
  // device; again, as a copy, on the server's player, which takes no reorder) as the large actions, and Remove from
  // queue last (this device, as the row's swipe).
  import ReplaceTrackSheet from './ReplaceTrackSheet.svelte';
  import { leavePlayer } from './navStep.svelte';
  import { port } from './player/port.svelte';
  import {
    coverUrl,
    albumCoverUrl,
    queueTrack,
    setTrackFavorite,
    setTrackMember,
    trackFile,
    trackState,
    getTrackVersions,
    type TrackFile,
    type TrackState,
    type TrackVersions,
  } from './api';
  import { playCollection } from './play.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { BANDCAMP, previewPage } from './bandcampPreview';
  import DiscoverEntries from './DiscoverEntries.svelte';
  import { heardOf, loadWaveform, waveformOf } from './waveforms.svelte';
  import { withHeard } from './trackFacts.svelte';
  import type { IconName } from './Icon.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import { goToArtist as showArtist, mainArtist } from './albums.svelte';
  import { player, refresh } from './live.svelte';
  import { excludeGlobally } from './exclusions.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import MenuTable from './MenuTable.svelte';
  import { inOrder, type EntryId } from './menuTable';
  import MenuEntry from './MenuEntry.svelte';
  import PlaylistToggles from './PlaylistToggles.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { shownCover, favorite, pending, playing, toggleFavorite } from './remote.svelte';
  import { enqueue, jump as localJump, moveUpcoming, removeUpcoming, startRadio } from './localPlayer.svelte';
  import { playUpcoming } from './remote.svelte';
  import { setPlayingMember, shownPlaylists } from './output.svelte';
  import { setFavorite } from './favourites.svelte';
  import { toast } from './toast.svelte';
  import { albumSubject, artistOf, panel, viewOver } from './panel.svelte';
  import { linkTo, share } from './share';
  import { crateOf, genreOf, viewGenre, viewLike } from './crates.svelte';
  import { me } from './me.svelte';
  import TempoStep from './TempoStep.svelte';
  import { askFor, followingNow } from './following.svelte';
  import { mine, setAutoAccept } from './mySettings.svelte';
  import { creditsOf } from './artistCredits';
  import { excludedArtists, excludedEverywhere, loadCrateArtists, loadExcludedArtists, notInThisCrate, setExcludedEverywhere, setNotInCrate } from './excludedArtists.svelte';

  let {
    onclose,
    track = null,
    start = 'menu',
    queue: place = null,
    onplaylists = null,
    outs = [],
    crate = null,
    asker = null,
  }: {
    onclose: () => void;
    track?: MenuTrack | null;
    start?: 'menu' | 'lists';
    queue?: QueuePlace | null;
    /** Opened from a component over the whole page (the full player): its playlists' step offers the Playlists page. */
    onplaylists?: (() => void) | null;
    /** Opened on a list one can take it out of: Remove from a playlist, Exclude from a crate (the track, its album), as
     *  a swipe left does there (the owner, 2026-10-19). */
    outs?: { label: string; icon: IconName; run: () => void }[];
    /** A track asked for of one's own player (0.10): the user who asked, whose tracks may be accepted at once. */
    asker?: string | null;
    /** Opened in a crate (its id): its artists Not in this crate (0.10, decision 0083). */
    crate?: string | null;
  } = $props();
  // Its artists left out everywhere or of the crate it is opened in (0.10, decision 0083): read as the menu opens.
  loadExcludedArtists();
  $effect(() => {
    if (crate) loadCrateArtists(crate);
  });
  /** Following someone (0.10): Play next and Add to queue ask them instead. */
  const followed = $derived(followingNow());
  async function ask(how: 'next' | 'last') {
    const tr = t;
    if (!tr) return;
    onclose();
    try {
      await askFor([{ source: tr.source, id: tr.id, name: tr.name, artist: tr.artist, album: tr.album, album_id: tr.album_id }], how);
    } catch (e) {
      toast.fail(e, "It couldn't be asked for");
    }
  }

  const np = $derived(playing());
  /** The menu is for what plays (no track given, or the given one plays); a queue row is always its own place. */
  const forPlaying = $derived(!place && (!track || (!!np && np.source === track.source && np.id === track.id)));
  const t = $derived<MenuTrack | null>(forPlaying ? np : track);
  /** Find a better copy open (lib/ReplaceTrackSheet), in the menu's place. */
  let betterOpen = $state(false);

  /** One of the device's own (local mode, decision 0052): none of Cratarr's (hearts, playlists, downloads, share,
   *  details); Play next, Add to queue, its album and artist, the web search. */
  const onDevice = $derived(t?.source === 'device');
  // Another track's playlists and heart, asked for when its menu opens.
  let other = $state<TrackState | null>(null);
  $effect(() => {
    if (forPlaying || !track || track.source === 'device') return;
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
  /** The track's album, for Go to album: as the track came, else as the server says. */
  const albumId = $derived(t?.album_id ?? (forPlaying ? null : (other?.album_id ?? null)));
  // Opened from a row of what the panel shows (a playlist's, an album's, a genre's, an artist's): what it shows already
  // is not offered again (from the player or the queue it is: they cover the panel).
  const fromPanel = $derived(!!track && !place && !forPlaying);
  const viewedArtist = $derived(artistOf(panel.viewed));
  const viewedGenre = $derived(genreOf(crateOf(panel.viewed)));
  function viewAlbum() {
    if (!t || !albumId) return;
    const subject = albumSubject(t.source, albumId);
    onclose();
    // From the full-screen player: it closes once the album shows (lib/navStep), or it would cover it.
    leavePlayer();
    viewOver(subject);
  }
  const heartOn = $derived(forPlaying ? favorite() : !!other?.favorite);
  const heartKnown = $derived(forPlaying ? np?.favorite !== null : other !== null && other.favorite !== null);
  const busy = $derived(forPlaying ? pending() : !other);
  // Playlists holding the track first, then the rest by name.
  // In the order its Add to playlist's header says (lib/PlaylistToggles).
  const playlists = $derived(forPlaying ? shownPlaylists() : (other?.playlists ?? []));

  // The step it opens on; later steps are the sheet's own.
  // svelte-ignore state_referenced_locally
  let step = $state<'menu' | 'lists' | 'details' | 'download' | 'tempo' | 'versions'>(start);
  // Other versions (0.10): the library's other recordings of its work (MusicBrainz's work links), asked as the menu
  // opens where the list says there are some, or does not say (a server without them answers 404: none shown).
  let versions = $state<TrackVersions | null>(null);
  $effect(() => {
    const now = t;
    versions = null;
    if (!now || now.source !== 'cratarr' || now.has_versions === false) return;
    let gone = false;
    getTrackVersions(now.source, now.id).then(
      (v) => !gone && v?.versions?.length && (versions = v),
      () => {},
    );
    return () => (gone = true);
  });
  /** A version tapped: it plays now. */
  function playVersion(v: TrackVersions['versions'][number]) {
    onclose();
    playCollection({ kind: 'tracks', source: v.source, ids: [v.id] }, 'play', v.title).catch((e) => toast.fail(e, 'Could not start playing'));
  }

  // Go to artist: the artist's panel (lib/albums goToArtist). A "feat." credit is left out: the albums are under the
  // main artist.
  const artistName = $derived(mainArtist(t?.artist));
  /** The artists its credit names, each on its own (feat., &, with…: decision 0083's split). */
  const credited = $derived(creditsOf(t?.artist));
  function goToArtist() {
    // Read before closing, as every action here: the opener drops the menu's track on close.
    const name = artistName;
    const source = t?.source;
    if (!name || !source) return;
    onclose();
    showArtist(source, name);
  }

  // Share: the track in words, and its album's link here (a track has no page of its own; lib/share.ts).
  function shareIt() {
    if (!t) return;
    const { source, name, artist, album_id } = t;
    onclose();
    // A Bandcamp preview's link is its album's page there, not one of Cratarr's (the owner, 2026-10-25).
    const url = bandcampPage ?? (album_id ? linkTo('library', albumSubject(source, album_id)) : null);
    share(name, [artist, name].filter(Boolean).join(' – '), url);
  }

  // Adjust BPM only where there is a waveform to adjust it on (the owner, 2026-10-25); a Bandcamp preview's, worked out
  // on the device, keeps no tempo of the server's to adjust.
  $effect(() => {
    if (t && me.admin && !onDevice && t.source !== BANDCAMP) loadWaveform(t.source, t.id);
  });
  const hasWave = $derived(!!t && t.source !== BANDCAMP && !!waveformOf(t.source, t.id));

  // A Bandcamp preview (decision 0071): its album's page, for its Discover group (lib/DiscoverEntries) and Share.
  const bandcampPage = $derived(t?.source === BANDCAMP ? previewPage(t.id) : null);

  // Web search: the track's artist and title, in a new tab (the app opens it outside).
  function webSearch() {
    if (!t) return;
    const words = [t.artist, t.name].filter(Boolean).join(' ');
    window.open(`https://duckduckgo.com/?q=${encodeURIComponent(words)}`, '_blank', 'noopener');
    onclose();
  }  // Feel (decision 0058): Exclude everywhere, as the heart's long-press has it (decision 0024, amended).
  function excludeEverywhere() {
    const one = t;
    if (!one) return;
    onclose();
    excludeGlobally(one.source, [one.id]).catch((e) => toast.fail(e, "It wasn't excluded"));
  }
  function geniusSearch() {
    if (!t) return;
    const words = [t.artist, t.name].filter(Boolean).join(' ');
    window.open(`https://genius.com/search?q=${encodeURIComponent(words)}`, '_blank', 'noopener');
    onclose();
  }

  // The file as it is, read as the menu opens: its genres for Go to genre, and Details.
  let details = $state<TrackFile | null>(null);
  let detailsFailed = $state(false);
  // Read again only for another track: the playing track's view is made anew at every report of the player (its
  // position), and reading on each made the genres' entries come and go (and asked the server again and again).
  const fileOf = $derived(t ? `${t.source}\n${t.id}` : null);
  $effect(() => {
    const key = fileOf;
    details = null;
    detailsFailed = false;
    if (!key) return;
    const [source, id] = key.split('\n');
    trackFile(source, id).then(
      (d) => {
        if (fileOf === key) details = d;
      },
      () => {
        if (fileOf === key) detailsFailed = true;
      },
    );
  });
  function showDetails() {
    step = 'details';
  }
  /** Go to genre: the track's first genres, each opening in the panel. */
  const genres = $derived((details?.genres ?? []).slice(0, 2));
  function goToGenre(name: string) {
    onclose();
    // From the full-screen player: it closes once the genre shows, or it would cover it.
    leavePlayer();
    viewGenre(name);
  }
  /** Sounds like it: a crate seeded with the track (Cratarr's own, which AudioMuse hears). */
  function goToLike() {
    // Read before closing: closing lets go of the track.
    const id = t!.id;
    onclose();
    leavePlayer();
    viewLike(id);
  }
  const plays = $derived(forPlaying ? (np?.info?.play_count ?? null) : (other?.info?.play_count ?? null));
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
  const day = (s: number) => new Date(s * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  function facts(file: TrackFile): [string, string][] {
    const d = (t && withHeard(file, heardOf(t.source, t.id))) || file;
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
    if (d.bpm) out.push(['Tempo', `${d.bpm} BPM`]);
    if (d.size) out.push(['Size', `${(d.size / 1e6).toFixed(1)} MB`]);
    if (plays !== null) out.push(['Plays', String(plays)]);
    if (d.added) out.push(['Added', day(d.added)]);
    // A Bandcamp preview has no file of Cratarr's: its page there (decision 0071).
    out.push(t?.source === BANDCAMP ? ['Bandcamp', d.path] : ['File', d.folder ? `${d.folder}/${d.path}` : d.path]);
    for (const tag of d.tags) out.push([tag.name, tag.value]);
    return out;
  }
  let working = $state(false);

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
  // The queue's own rows move where the page holds it.
  const here = $derived(port().queue() !== null);
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

  /** Radio from this track (decision 0036): tracks like it play, and the radio goes on. Cratarr's own tracks only. */
  const radioFrom = $derived(port().radio && t?.source === 'cratarr');
  async function radio() {
    const tr = t;
    if (!tr) return;
    working = true;
    onclose();
    try {
      await startRadio([tr.id], tr.name);
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    } finally {
      working = false;
    }
  }

  async function queue(how: 'next' | 'last') {
    const tr = t;
    if (!tr) return;
    working = true;
    onclose();
    try {
      await port().enqueueOne({ source: tr.source, id: tr.id, name: tr.name, artist: tr.artist, album: tr.album, album_id: tr.album_id }, how);
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
</script>

{#snippet entry(e: EntryId)}
  {#if !t}
    <!-- Nothing to show. -->
  {:else if e === 'goAlbum'}
    {#if albumId && !(fromPanel && panel.viewed === albumSubject(t.source, albumId))}
      <MenuEntry icon="squareArrowOutUpRight" label="Go to album" onclick={viewAlbum} />
    {/if}
  {:else if e === 'goArtist'}
    {#if artistName && !(fromPanel && viewedArtist?.source === t.source && viewedArtist?.name === artistName)}
      <MenuEntry icon="user" label="Go to {artistName}" onclick={goToArtist} />
    {/if}
  {:else if e === 'goGenre'}
    {#each genres.filter((g) => !(fromPanel && g === viewedGenre)) as g (g)}
      <MenuEntry icon="music" label="Go to {g}" onclick={() => goToGenre(g)} />
    {/each}
  {:else if e === 'soundsLike'}
    {#if t.source === 'cratarr'}<MenuEntry icon="music" label="Sounds like this" onclick={goToLike} />{/if}
  {:else if e === 'otherVersions'}
    {#if versions}<MenuEntry icon="music" label="Other versions" onclick={() => (step = 'versions')} />{/if}
  {:else if e === 'discover'}
    <DiscoverEntries page={bandcampPage} artist={t.artist} album={t.album} {onclose} />
  {:else if e === 'favourite'}
    {#if heartKnown}<MenuEntry icon="heart" label="Favourite" switched={heartOn} disabled={busy} onclick={heart} />{/if}
  {:else if e === 'excludeEverywhere'}
    {#if !onDevice}<MenuEntry icon="ban" label="Exclude everywhere" danger onclick={excludeEverywhere} />{/if}
  {:else if e === 'excludeArtist'}
    {#if !onDevice && t.source !== BANDCAMP && excludedArtists.everywhere !== null}
      {#each credited as a (a)}
        {@const on = excludedEverywhere(a)}
        <MenuEntry icon="ban" label="Exclude {a} everywhere" switched={on} onclick={() => setExcludedEverywhere(a, !on)} />
      {/each}
    {/if}
  {:else if e === 'notInCrate'}
    {#if crate && excludedArtists.crates[crate]}
      {#each credited as a (a)}
        {@const out = notInThisCrate(crate, a)}
        <MenuEntry icon="crateOut" label="{a} not in this crate" switched={out} onclick={() => setNotInCrate(crate, a, !out)} />
      {/each}
    {/if}
  {:else if e === 'addToPlaylist'}
    {#if addable}<MenuEntry icon="playlists" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
  {:else if e === 'download'}
    {#if !onDevice && t.source !== BANDCAMP}<MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (step = 'download')} />{/if}
  {:else if e === 'share'}
    {#if !onDevice}<MenuEntry icon="share" label="Share" onclick={shareIt} />{/if}
  {:else if e === 'info'}
    {#if !onDevice}<MenuEntry icon="info" label="Info" onclick={showDetails} />{/if}
  {:else if e === 'webSearch'}
    <MenuEntry icon="search" label="Web search" onclick={webSearch} />
  {:else if e === 'geniusSearch'}
    <MenuEntry icon="search" label="Genius search" onclick={geniusSearch} />
  {:else if e === 'adjustBpm'}
    {#if me.admin && !onDevice && hasWave}<MenuEntry icon="gauge" label="Adjust BPM" onclick={() => (step = 'tempo')} />{/if}
  {:else if e === 'betterCopy'}
    {#if me.admin && t.source === 'cratarr' && t.album_id}<MenuEntry icon="search" label="Find a better copy…" onclick={() => (betterOpen = true)} />{/if}
  {:else if e === 'alwaysAccept'}
    {#if asker}
      {@const on = mine.autoAccept.includes(asker)}
      <MenuEntry icon="thumbsUp" label={`Always accept from ${asker}`} switched={on} onclick={() => void setAutoAccept(asker, !on)} />
    {/if}
  {:else if e === 'removeFromQueue'}
    {#if movesHere && !forPlaying}<MenuEntry icon="trash" label="Remove from queue" danger onclick={removeFromQueue} />{/if}
  {:else if e === 'removeFromList'}
    {#each outs as o (o.label)}
      <MenuEntry
        icon={o.icon}
        label={o.label}
        danger
        onclick={() => {
          onclose();
          o.run();
        }}
      />
    {/each}
  {/if}
{/snippet}

{#if t && betterOpen && t.album_id}
  <!-- A better copy of it (0.9.8, lib/ReplaceTrackSheet): the menu gives way to it. -->
  <ReplaceTrackSheet source={t.source} album={{ id: t.album_id, title: t.album ?? '' }} replace={{ source: t.source, id: t.id }} {onclose} />
{:else if t}
  <MenuSheet
    label={t.name}
    {onclose}
    images={src ? [src] : []}
    title={t.name}
    chips={[t.artist, t.album].filter((c): c is string => !!c)}
    heart={heartKnown ? { on: heartOn, disabled: busy, toggle: heart } : null}
    actions={step === 'download'
      ? []
      : step === 'lists' && onplaylists
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
        ? inOrder('track', [
            { icon: 'play', label: 'Play now', onclick: playNow, disabled: working },
            { icon: 'playNext', label: movesHere ? 'Play next' : 'Play again next', onclick: playNextFromQueue, disabled: working },
            ...(radioFrom ? [{ icon: 'radio' as const, label: 'Start radio', onclick: radio, disabled: working }] : []),
          ])
        : followed
        ? inOrder('track', [
            { icon: 'playNext', label: 'Ask to play next', onclick: () => ask('next'), disabled: working },
            { icon: 'listAdd', label: 'Ask to add to queue', onclick: () => ask('last'), disabled: working },
          ])
        : inOrder('track', [
            { icon: 'playNext', label: 'Play next', onclick: () => queue('next'), disabled: working },
            { icon: 'listAdd', label: 'Add to queue', onclick: () => queue('last'), disabled: working },
            ...(radioFrom ? [{ icon: 'radio' as const, label: 'Start radio', onclick: radio, disabled: working }] : []),
          ])}
    onback={step === 'menu' ? null : () => (step = 'menu')}
    {step}
  >
    {#if step === 'menu'}
      <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
      <MenuTable kind="track" {entry} />
    {:else if step === 'download'}
      <DownloadStep what={{ kind: 'tracks', name: t.name, tracks: [{ source: t.source, id: t.id }] }} tracks={[t]} {onclose} />
    {:else if step === 'tempo'}
      <TempoStep source={t.source} tracks={[{ id: t.id, name: t.name }]} />
    {:else if step === 'lists'}
      <PlaylistToggles title="Add to / remove from playlists" {playlists} ticked={(p) => p.member}>
        {#snippet entry(p)}<MenuEntry label={p.name} checked={p.member} disabled={busy} onclick={() => toggle(p.id, p.member)} />{/snippet}
      </PlaylistToggles>
    {:else if step === 'versions' && versions}
      {@const v = versions}
      <p class="work">{v.work.title}{#if v.composers.length} · {v.composers.map((c) => c.name).join(', ')}{/if}</p>
      <MenuCards>
        <MenuGroup label="Other versions">
          {#each v.versions as o (o.source + o.id)}
            <MenuEntry
              icon="play"
              label={o.performers?.length ? o.performers.join(' · ') : (o.artist ?? o.title)}
              sub={[o.title, o.album, o.year ? String(o.year) : null].filter(Boolean).join(' · ')}
              onclick={() => playVersion(o)}
            />
          {/each}
        </MenuGroup>
      </MenuCards>
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
    color: var(--danger-ink);
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }
  /* The work Other versions are of, and its composers, over them. */
  .work {
    margin: 0 4px 10px;
    color: var(--muted);
  }
</style>
