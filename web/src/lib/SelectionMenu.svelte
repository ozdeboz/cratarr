<script lang="ts">
  // The ticked playlists' menu (decision 0025, the ⋮ beside Select's Cancel): a sheet as every menu (decision 0021),
  // Play, Shuffle, Play next and Add to queue as the large actions (their tracks one playlist after another, in the
  // order ticked, each once), then favourite, pin, and Delete (a step to confirm; one Undo brings all back).
  import { deletePlaylist, restorePlaylist, type PlayMode } from './api';
  import { isPlaylistFavorite, setPlaylistFavorites } from './favourites.svelte';
  import { insights } from './insights.svelte';
  import { refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { startPlaylists } from './output.svelte';
  import { playlistDeleted } from './panel.svelte';
  import { prefs } from './prefs.svelte';
  import { selectDeleted } from './select.svelte';
  import { toast } from './toast.svelte';

  let { ids, onclose }: { ids: string[]; onclose: () => void } = $props();

  let mode = $state<'menu' | 'delete'>('menu');
  let working = $state(false);
  const n = $derived(ids.length);
  const title = $derived(n === 1 ? '1 playlist' : `${n} playlists`);
  const allFav = $derived(ids.every((id) => isPlaylistFavorite(id)));
  const allPinned = $derived(ids.every((id) => prefs.pinned.includes(id)));

  async function play(how: PlayMode) {
    if (working) return;
    working = true;
    const list = [...ids];
    onclose();
    try {
      await startPlaylists(list, how);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      working = false;
    }
  }

  function pin() {
    prefs.pinned = allPinned ? prefs.pinned.filter((x) => !ids.includes(x)) : [...prefs.pinned, ...ids.filter((x) => !prefs.pinned.includes(x))];
    onclose();
  }

  async function remove() {
    if (working) return;
    working = true;
    const gone: string[] = [];
    try {
      for (const id of ids) {
        await deletePlaylist(id);
        gone.push(id);
        playlistDeleted(id);
        selectDeleted(id);
        insights.deleted(id);
      }
      onclose();
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
    if (!gone.length) return;
    toast.show({
      text: gone.length === 1 ? `Deleted ${gone[0]}.` : `Deleted ${gone.length} playlists.`,
      undo: async () => {
        try {
          for (const id of gone) {
            await restorePlaylist(id);
            insights.restored(id);
          }
        } catch (e) {
          toast.fail(e, 'Could not restore the playlists');
        } finally {
          refresh();
        }
      },
    });
  }
</script>

<MenuSheet
  label={title}
  {onclose}
  {title}
  chips={ids.slice(0, 3).concat(n > 3 ? [`and ${n - 3} more`] : [])}
  actions={mode === 'menu'
    ? [
        { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: working },
        { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: working },
        { icon: 'playNext', label: 'Play next', onclick: () => play('next'), disabled: working },
        { icon: 'playlists', label: 'Add to queue', onclick: () => play('last'), disabled: working },
      ]
    : []}
  onback={mode === 'menu' ? null : () => (mode = 'menu')}
  step={mode}
>
  {#if mode === 'menu'}
    <div role="menu">
      <MenuEntry
        icon="heart"
        label={allFav ? 'Remove from favourites' : 'Add to favourites'}
        onclick={() => {
          setPlaylistFavorites([...ids], !allFav);
          onclose();
        }}
      />
      <MenuEntry icon="pin" label={allPinned ? 'Unpin' : 'Pin to top'} onclick={pin} />
      <MenuEntry icon="trash" label="Delete" danger onclick={() => (mode = 'delete')} />
    </div>
  {:else}
    <div class="form">
      <p class="ask">Delete {title}? You can undo this right after.</p>
      <button class="danger" disabled={working} aria-busy={working} onclick={remove}>{n === 1 ? 'Delete playlist' : `Delete ${n} playlists`}</button>
    </div>
  {/if}
</MenuSheet>

<style>
  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .ask {
    margin: 0;
    font-size: 0.95rem;
    text-align: center;
  }

  .danger {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--danger);
    color: #fff;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .danger:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
