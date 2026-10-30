<script lang="ts">
  // An artist's menu (the artist panel's ⋮, decided 2026-10-16, after Symfonium's): the artist at the top (their
  // heart there), the large actions (Play, Shuffle, Play next, Add to queue), then on rounded cards Start radio (their
  // Radio mix), then in the table's order (lib/menuTable) Add to playlist (all their tracks, as an album's), Download…,
  // in the app Sync to this device (decision 0038), Share, and their picture on this device (one chosen here, or their
  // newest album's cover again). No Instant mix (the owner: "a confusing thing"), no rating (the server keeps none for
  // artists).
  import { albumCoverUrl, search } from './api';
  import { likeChip } from './quickCrate';
  import { grabCrate, openQuickCrate } from './quickCrates.svelte';
  import { downloads, inApp, removeDownloads } from './app.svelte';
  import { artistPictureId } from './albums.svelte';
  import { coverKey, localCovers, removeLocalCover, setLocalCover } from './localCovers.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import { isKept, setKept } from './kept.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuTable from './MenuTable.svelte';
  import { inOrder, type EntryId } from './menuTable';
  import { BANDCAMP } from './bandcampPreview';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { artistSubject } from './panel.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { linkTo, share } from './share';
  import { toast } from './toast.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';
  import { excludedArtists, excludedEverywhere, loadCrateArtists, loadExcludedArtists, notInThisCrate, setExcludedEverywhere, setNotInCrate } from './excludedArtists.svelte';

  type Act = 'play' | 'shuffle' | 'next' | 'last';
  let {
    source,
    name,
    cover,
    albums,
    tracks,
    favorite,
    ontogglefavorite,
    onact,
    onradio,
    crate = null,
    onclose,
  }: {
    source: string;
    name: string;
    /** Their picture as the panel shows it. */
    cover: string | null;
    albums: number;
    /** Their tracks, album by album. */
    tracks: LocalTrack[];
    /** The heart; undefined where the server keeps no favourite artists. */
    favorite?: boolean;
    ontogglefavorite: () => void;
    /** The panel's action bar's own (lib/PlaylistView). */
    onact: (mode: Act) => void;
    /** Start radio, where it can (Cratarr's own player). */
    onradio?: () => void;
    /** Opened from a crate (its id): Not in this crate (0.10, decision 0083). */
    crate?: string | null;
    onclose: () => void;
  } = $props();
  // Left out everywhere or of the crate it is opened from (0.10, decision 0083): read as the menu opens.
  loadExcludedArtists();
  $effect(() => {
    if (crate) loadCrateArtists(crate);
  });

  const ids = $derived(tracks.map((t) => t.id));
  const actions = $derived.by<SheetAction[]>(() => {
    if (exporting || step !== 'menu') return [];
    const act = (mode: Act) => () => {
      onclose();
      onact(mode);
    };
    const none = !tracks.length;
    return inOrder('artist', [
      { icon: 'play', label: 'Play', disabled: none, onclick: act('play') },
      { icon: 'shuffle', label: 'Shuffle', disabled: none, onclick: act('shuffle') },
      { icon: 'playNext', label: 'Play next', disabled: none, onclick: act('next') },
      { icon: 'listAdd', label: 'Add to queue', disabled: none, onclick: act('last') },
      ...(onradio
        ? [
            {
              icon: 'radio' as const,
              label: 'Start radio',
              disabled: none,
              onclick: () => {
                onclose();
                onradio?.();
              },
            },
          ]
        : []),
    ]);
  });

  // Add to playlist (lib/tracksToPlaylist): all their tracks; the first one's state says whether they can go in one.
  const toList = new TracksToPlaylist(
    () => source,
    () => ids,
    // svelte-ignore state_referenced_locally
    `all of ${name}`,
  );
  $effect(() => {
    if (ids[0]) toList.ask([ids[0]]);
  });
  let step = $state<ToPlaylistStep>('menu');
  /** Download… (decision 0058, amended 0.10): their tracks, in lib/DownloadStep. */
  let exporting = $state(false);

  /** Bandcamp's previews (an artist found there): never downloaded or kept (decision 0071). */
  const previews = $derived(tracks.some((t) => t.source === BANDCAMP));
  /** Sounds like them (0.12; one way to make a crate): the crate search's sentence with the artist as its "like" term,
   *  by the library's id search gives them; where search does not name them, their name read as a line. */
  function soundsLike() {
    const n = name;
    onclose();
    search(n, ['artist'], 5).then(
      (r) => {
        const id = r.groups.artist?.items.find((x) => x.title.toLowerCase() === n.toLowerCase())?.id;
        if (id) grabCrate([likeChip('artist', id, n)]);
        else openQuickCrate(n);
      },
      () => openQuickCrate(n),
    );
  }
  /** Their tracks kept on this device (the app's downloads): Remove from this device. */
  const kept = $derived(inApp ? downloads.tracks.filter((t) => t.source === source && ids.includes(t.id)) : []);

  function shareIt() {
    onclose();
    share(name, name, linkTo('library', artistSubject(source, name)));
  }
  // Know (decision 0058): a web search on their name, as an album's.
  function webSearch() {
    onclose();
    window.open(`https://duckduckgo.com/?q=${encodeURIComponent(name)}`, '_blank', 'noopener');
  }

  // Their picture on this device (lib/localCovers, as an album's cover): one chosen here, or none (their newest
  // album's cover again).
  const ownPicture = $derived(!!localCovers[coverKey(source, artistPictureId(name))]);
  let picker = $state<HTMLInputElement>();
  async function chosen(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    onclose();
    if (!file) return;
    try {
      await setLocalCover(source, artistPictureId(name), file);
    } catch (e) {
      toast.fail(e, "The picture wasn't changed");
    }
  }
  async function dropPicture() {
    onclose();
    try {
      await removeLocalCover(source, artistPictureId(name));
    } catch (e) {
      toast.fail(e, "The picture wasn't changed");
    }
  }
</script>

{#snippet entry(e: EntryId)}
  {#if e === 'soundsLike'}
    {#if source === 'cratarr' && !previews}<MenuEntry icon="music" label="Sounds like this" onclick={soundsLike} />{/if}
  {:else if e === 'favourite'}
    {#if favorite !== undefined}<MenuEntry icon="heart" label="Favourite" switched={favorite} onclick={ontogglefavorite} />{/if}
  {:else if e === 'excludeArtist'}
    {#if excludedArtists.everywhere !== null}
      {@const on = excludedEverywhere(name)}
      <MenuEntry icon="ban" label="Exclude everywhere" switched={on} onclick={() => setExcludedEverywhere(name, !on)} />
    {/if}
  {:else if e === 'notInCrate'}
    {#if crate && excludedArtists.crates[crate]}
      {@const out = notInThisCrate(crate, name)}
      <MenuEntry icon="crateOut" label="Not in this crate" switched={out} onclick={() => setNotInCrate(crate, name, !out)} />
    {/if}
  {:else if e === 'addToPlaylist'}
    {#if toList.addable}<MenuEntry icon="playlists" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
  {:else if e === 'download'}
    {#if tracks.length && !previews}<MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (exporting = true)} />{/if}
  {:else if e === 'keep'}
    {#if inApp && !previews}
      <!-- Kept on this device (decision 0038): their tracks stay in step offline. -->
      {@const on = isKept('artist', source, name)}
      <MenuEntry icon="download" label="Sync to this device" switched={on} onclick={() => setKept('artist', source, name, !on, name)} />
    {/if}
  {:else if e === 'removeFromDevice'}
    <!-- Beside what put them there (the owner, 2026-10-26: removing was out of sight, last in the menu). -->
    {#if kept.length}
      {@const keptIds = kept.map((t) => t.download.id)}
      <MenuEntry
        icon="trash"
        label="Remove from this device"
        danger
        onclick={() => {
          onclose();
          removeDownloads(keptIds);
        }}
      />
    {/if}
  {:else if e === 'share'}
    <MenuEntry icon="share" label="Share" onclick={shareIt} />
  {:else if e === 'webSearch'}
    <MenuEntry icon="search" label="Web search" onclick={webSearch} />
  {:else if e === 'devicePicture'}
    <MenuEntry icon="image" label="Change picture on this device" onclick={() => picker?.click()} />
  {:else if e === 'albumPicture'}
    {#if ownPicture}<MenuEntry icon="imageOff" label="Use their album cover" onclick={dropPicture} />{/if}
  {/if}
{/snippet}

<MenuSheet
  label={name}
  {onclose}
  images={cover ? [cover] : []}
  title={name}
  chips={[albums === 1 ? '1 album' : `${albums} albums`]}
  heart={exporting || step !== 'menu' || favorite === undefined ? null : { on: favorite, toggle: ontogglefavorite }}
  {actions}
  onback={exporting ? () => (exporting = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={exporting ? 'export' : step}
>
  {#if exporting}
    <DownloadStep what={{ kind: 'tracks', name, tracks: tracks.map((x) => ({ source: x.source, id: x.id })) }} {tracks} {onclose} />
  {:else if step === 'menu'}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable kind="artist" {entry}>
      <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
    </MenuTable>
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>

<style>
  .picker {
    display: none;
  }
</style>
