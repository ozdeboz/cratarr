<script lang="ts">
  // The menu of what is ticked (decision 0058: a selection is an object, its menu the same as one's; the select bars'
  // ⋮ at their far right): several tracks, or several albums' tracks. Play, Shuffle, Play next and Add to queue as the
  // large actions (the server's choice of them, 500 at most: lib/play), and Start radio from them all (Cratarr's own,
  // played here: lib/TrackMenu's); then in the table's order (lib/menuTable) Exclude everywhere (one Undo for all); Add
  // to playlist; Download…; Adjust BPM (administrators: lib/TempoStep). `extra`: what the list's own items have
  // besides their tracks (several albums' Keep, Delete…; the owner, 2026-10-27: what one has, several have), its
  // groups last, given the menu's close; `chips`: what is ticked, said as the list says it ("5 albums").
  import MenuSheet from './MenuSheet.svelte';
  import { port } from './player/port.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuTable from './MenuTable.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import { inOrder, type EntryId } from './menuTable';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { me } from './me.svelte';
  import TempoStep from './TempoStep.svelte';
  import { startRadio, type LocalTrack } from './localPlayer.svelte';
  import { playCollection } from './play.svelte';
  import { excludeGlobally } from './exclusions.svelte';
  import { askIfMany } from './many.svelte';
  import { plural } from './insights.svelte';
  import { toast } from './toast.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';
  import type { Snippet } from 'svelte';

  let {
    title,
    tracks,
    chips = null,
    extra,
    onclose,
  }: { title: string; tracks: LocalTrack[]; chips?: string[] | null; extra?: Snippet<[() => void]>; onclose: () => void } = $props();

  const source = $derived(tracks[0]?.source ?? '');
  const ids = $derived(tracks.map((t) => t.id));
  const toList = new TracksToPlaylist(
    () => source,
    () => ids,
    // svelte-ignore state_referenced_locally
    title,
  );
  $effect(() => {
    if (ids[0]) toList.ask([ids[0]]);
  });
  let step = $state<ToPlaylistStep>('menu');
  let exporting = $state(false);
  let tempo = $state(false);
  let working = $state(false);

  async function play(how: 'play' | 'shuffle' | 'next' | 'last') {
    const [s, list, name] = [source, [...ids], title];
    working = true;
    try {
      await playCollection({ kind: 'tracks', source: s, ids: list }, how, name);
      if (how === 'next' || how === 'last') toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      onclose();
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      working = false;
    }
  }

  // What one track's menu has, several have (the owner, 2026-10-27): Start radio and Exclude everywhere, each one call.
  const radioFrom = $derived(port().radio && source === 'cratarr');
  async function radio() {
    const [list, name] = [[...ids], title];
    working = true;
    onclose();
    try {
      await startRadio(list, name);
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    } finally {
      working = false;
    }
  }
  /** Many ask first (lib/many). */
  function excludeEverywhere() {
    const [s, list, n] = [source, [...ids], plural(ids.length, 'track')];
    onclose();
    askIfMany(list.length, { title: `Exclude ${n} everywhere?`, label: `Exclude ${n}`, icon: 'ban' }, () =>
      void excludeGlobally(s, list).catch((e) => toast.fail(e, "They weren't excluded")),
    );
  }
  // Bandcamp's previews are never downloaded (decision 0071).
  const downloadable = $derived(!!source && source !== 'device' && source !== BANDCAMP && tracks.length > 0);
</script>

{#snippet entry(e: EntryId)}
  {#if e === 'excludeEverywhere'}
    {#if source && source !== 'device'}<MenuEntry icon="ban" label="Exclude everywhere" danger onclick={excludeEverywhere} />{/if}
  {:else if e === 'addToPlaylist'}
    {#if toList.addable}<MenuEntry icon="playlists" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
  {:else if e === 'download'}
    {#if downloadable}<MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (exporting = true)} />{/if}
  {:else if e === 'adjustBpm'}
    {#if me.admin && source !== 'device' && source !== BANDCAMP}<MenuEntry icon="gauge" label="Adjust BPM" onclick={() => (tempo = true)} />{/if}
  {:else if e === 'extra'}
    {@render extra?.(onclose)}
  {/if}
{/snippet}

<MenuSheet
  label={title}
  {onclose}
  {title}
  chips={chips ?? [plural(tracks.length, 'track')]}
  actions={exporting || tempo || step !== 'menu'
    ? []
    : inOrder('selection', [
        { icon: 'play', label: 'Play', disabled: working || !ids.length, onclick: () => play('play') },
        { icon: 'shuffle', label: 'Shuffle', disabled: working || !ids.length, onclick: () => play('shuffle') },
        { icon: 'playNext', label: 'Play next', disabled: working || !ids.length, onclick: () => play('next') },
        { icon: 'listAdd', label: 'Add to queue', disabled: working || !ids.length, onclick: () => play('last') },
        ...(radioFrom ? [{ icon: 'radio' as const, label: 'Start radio', disabled: working || !ids.length, onclick: radio }] : []),
      ])}
  onback={exporting ? () => (exporting = false) : tempo ? () => (tempo = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={exporting ? 'export' : tempo ? 'tempo' : step}
>
  {#if exporting}
    <DownloadStep what={{ kind: 'tracks', name: title, tracks: tracks.map((t) => ({ source: t.source, id: t.id })) }} {tracks} {onclose} />
  {:else if tempo}
    <TempoStep {source} tracks={tracks.map((t) => ({ id: t.id, name: t.name }))} />
  {:else if step === 'menu'}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable kind="selection" {entry} />
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>
