<script lang="ts">
  // The menu of what is ticked (decision 0058: a selection is an object, its menu the same as one's; the select bars'
  // ⋮ at their far right): several tracks, or several albums' tracks. Play, Shuffle, Play next and Add to queue as the
  // large actions (the server's choice of them, 500 at most: lib/play); then Add to playlist; Download (in the app),
  // Export….
  import MenuSheet from './MenuSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import ExportStep from './ExportStep.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { download, inApp } from './app.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import { playCollection } from './play.svelte';
  import { plural } from './insights.svelte';
  import { toast } from './toast.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';

  let { title, tracks, onclose }: { title: string; tracks: LocalTrack[]; onclose: () => void } = $props();

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
</script>

<MenuSheet
  label={title}
  {onclose}
  {title}
  chips={[plural(tracks.length, 'track')]}
  actions={exporting || step !== 'menu'
    ? []
    : [
        { icon: 'play', label: 'Play', disabled: working || !ids.length, onclick: () => play('play') },
        { icon: 'shuffle', label: 'Shuffle', disabled: working || !ids.length, onclick: () => play('shuffle') },
        { icon: 'playNext', label: 'Play next', disabled: working || !ids.length, onclick: () => play('next') },
        { icon: 'listAdd', label: 'Add to queue', disabled: working || !ids.length, onclick: () => play('last') },
      ]}
  onback={exporting ? () => (exporting = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={exporting ? 'export' : step}
>
  {#if exporting}
    <ExportStep what={{ kind: 'tracks', name: title, tracks: tracks.map((t) => ({ source: t.source, id: t.id })) }} {onclose} />
  {:else if step === 'menu'}
    <!-- The shared groups, in their order (decision 0058). -->
    <MenuCards>
      <MenuGroup>
        {#if toList.addable}<MenuEntry icon="playlists" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
      </MenuGroup>
      <MenuGroup>
        {#if inApp}
          <MenuEntry
            icon="arrowDownToLine"
            label="Download"
            onclick={() => {
              download(tracks);
              onclose();
            }}
          />
        {/if}
        <MenuEntry icon="download" label="Export…" onclick={() => (exporting = true)} />
      </MenuGroup>
    </MenuCards>
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>
