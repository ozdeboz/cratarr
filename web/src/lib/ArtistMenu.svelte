<script lang="ts">
  // An artist's menu (the artist panel's ⋮, decided 2026-10-16, after Symfonium's): the artist at the top (their
  // heart there), the large actions (Play, Shuffle, Play next, Add to queue), then on rounded cards Start radio (their
  // Radio mix), Add to playlist (all their tracks, as an album's), Download and Share, and their picture on this device
  // (one chosen here, or their newest album's cover again). No Instant mix (the owner: "a confusing thing"), no rating
  // (the server keeps none for artists).
  import { albumCoverUrl } from './api';
  import { download, downloads, inApp, removeDownloads } from './app.svelte';
  import { artistPictureId } from './albums.svelte';
  import { coverKey, localCovers, removeLocalCover, setLocalCover } from './localCovers.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { artistSubject } from './panel.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { linkTo, share } from './share';
  import { toast } from './toast.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';

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
    onclose: () => void;
  } = $props();

  const ids = $derived(tracks.map((t) => t.id));
  const actions = $derived.by<SheetAction[]>(() => {
    if (step !== 'menu') return [];
    const act = (mode: Act) => () => {
      onclose();
      onact(mode);
    };
    const none = !tracks.length;
    return [
      { icon: 'play', label: 'Play', disabled: none, onclick: act('play') },
      { icon: 'shuffle', label: 'Shuffle', disabled: none, onclick: act('shuffle') },
      { icon: 'playNext', label: 'Play next', disabled: none, onclick: act('next') },
      { icon: 'listAdd', label: 'Add to queue', disabled: none, onclick: act('last') },
    ];
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

  /** Their tracks kept on this device (the app's downloads): Remove from this device. */
  const kept = $derived(inApp ? downloads.tracks.filter((t) => t.source === source && ids.includes(t.id)) : []);

  function shareIt() {
    onclose();
    share(name, name, linkTo('library', artistSubject(source, name)));
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

<MenuSheet
  label={name}
  {onclose}
  images={cover ? [cover] : []}
  title={name}
  chips={[albums === 1 ? '1 album' : `${albums} albums`]}
  heart={step !== 'menu' || favorite === undefined ? null : { on: favorite, toggle: ontogglefavorite }}
  {actions}
  onback={step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  {step}
>
  {#if step === 'menu'}
    <MenuCards>
      {#if onradio}
        {@const radio = onradio}
        <MenuGroup>
          <MenuEntry
            icon="radio"
            label="Start radio"
            onclick={() => {
              onclose();
              radio();
            }}
          />
        </MenuGroup>
      {/if}
      <MenuGroup>
        {#if toList.addable}<MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
        {#if inApp && tracks.length && kept.length < tracks.length}
          <MenuEntry
            icon="arrowDownToLine"
            label="Download"
            onclick={() => {
              onclose();
              download(tracks);
            }}
          />
        {/if}
        {#if kept.length}
          {@const keptIds = kept.map((t) => t.download.id)}
          <MenuEntry
            icon="trash"
            label="Remove from this device"
            onclick={() => {
              onclose();
              removeDownloads(keptIds);
            }}
          />
        {/if}
        <MenuEntry icon="share" label="Share" onclick={shareIt} />
      </MenuGroup>
      <MenuGroup>
        <MenuEntry icon="image" label="Change picture on this device" onclick={() => picker?.click()} />
        {#if ownPicture}<MenuEntry icon="imageOff" label="Use their album cover" onclick={dropPicture} />{/if}
      </MenuGroup>
      <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
    </MenuCards>
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>

<style>
  .picker {
    display: none;
  }
</style>
