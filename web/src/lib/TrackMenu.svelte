<script lang="ts">
  // The playing track's menu (decisions 0019, 0021): the menu button in the player, or a right-click on its cover. A
  // sheet as Finamp's track menu: the track on the card (with the heart), Play next and Add to queue as the large
  // actions, then: Add to playlist (the playlists as toggle cards, a tap adds or removes), the favourite, and a new
  // playlist made of what plays and what comes next.
  import { playlistFromQueue, queueTrack, setMember } from './api';
  import { live, refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs } from './prefs.svelte';
  import { coverSrc, favorite, playing, toggleFavorite } from './remote.svelte';
  import { enqueue } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { toast } from './toast.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const np = $derived(playing());
  const src = $derived(coverSrc());
  // Playlists holding the track first, then the rest by name.
  const playlists = $derived(
    [...(live.view?.playlists ?? [])].sort((a, b) => Number(b.member) - Number(a.member) || a.name.localeCompare(b.name)),
  );

  let step = $state<'menu' | 'lists' | 'new'>('menu');
  let working = $state(false);
  let nameField = $state('');

  async function queue(how: 'next' | 'last') {
    if (!np) return;
    working = true;
    onclose();
    try {
      if (isLocal()) enqueue([{ source: np.source, id: np.id, name: np.name, artist: np.artist, album: np.album, album_id: np.album_id }], how);
      else await queueTrack(np.id, how, np.source);
      toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      setTimeout(refresh, 700);
    } catch (e) {
      toast.fail(e, how === 'next' ? "It couldn't be put next" : "It couldn't be added to the queue");
    } finally {
      working = false;
    }
  }

  async function toggle(id: string, member: boolean) {
    if (!np || live.pending) return;
    try {
      live.view = await setMember(id, !member, prefs.source || undefined);
    } catch (e) {
      toast.fail(e, member ? "It couldn't be taken out" : "It couldn't be added");
    }
  }

  async function makePlaylist() {
    const name = nameField.trim();
    if (!name || working) return;
    working = true;
    try {
      const made = await playlistFromQueue(name, np?.source);
      const left = made.left_out ? `, ${made.left_out} not in the library left out` : '';
      toast.show({ text: `Made ${made.playlist}: ${made.added} ${made.added === 1 ? 'track' : 'tracks'}${left}.`, tone: 'done' });
      onclose();
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
    } finally {
      working = false;
    }
  }
</script>

{#if np}
  <MenuSheet
    label={np.name}
    {onclose}
    images={src ? [src] : []}
    title={np.name}
    chips={[np.artist, np.album].filter((c): c is string => !!c)}
    heart={np.favorite === null ? null : { on: favorite(), disabled: live.pending, toggle: toggleFavorite }}
    actions={step === 'menu'
      ? [
          { icon: 'playNext', label: 'Play next', onclick: () => queue('next'), disabled: working },
          { icon: 'playlists', label: 'Add to queue', onclick: () => queue('last'), disabled: working },
        ]
      : []}
    onback={step === 'menu' ? null : () => (step = 'menu')}
    {step}
  >
    {#if step === 'menu'}
      <div role="menu">
        {#if np.addable}
          <MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />
        {/if}
        {#if np.favorite !== null}
          <MenuEntry icon="heart" label={favorite() ? 'Remove from favourites' : 'Add to favourites'} disabled={live.pending} onclick={toggleFavorite} />
        {/if}
        <MenuEntry
          icon="plus"
          label="New playlist from the queue"
          onclick={() => {
            nameField = '';
            step = 'new';
          }}
        />
      </div>
    {:else if step === 'lists'}
      <div class="heading">Add to / remove from playlists</div>
      <div role="group" aria-label="Playlists">
        {#each playlists as p (p.id)}
          <MenuEntry label={p.name} checked={p.member} disabled={live.pending} onclick={() => toggle(p.id, p.member)} />
        {/each}
      </div>
    {:else}
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          makePlaylist();
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
