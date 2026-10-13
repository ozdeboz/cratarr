<script lang="ts">
  // The playing track's menu in the player (decision 0019): ☰, or a right-click on the cover. Play next, add to queue,
  // the favourite, the playlists that hold it (a tap adds or removes, as on the Playlists page), and a new playlist
  // made of what plays and what comes next.
  import { playlistFromQueue, queueTrack, setMember } from './api';
  import { live, refresh } from './live.svelte';
  import Popover from './Popover.svelte';
  import { prefs } from './prefs.svelte';
  import { favorite, toggleFavorite } from './remote.svelte';
  import { toast } from './toast.svelte';

  let { anchor, onclose }: { anchor: DOMRect; onclose: () => void } = $props();

  const np = $derived(live.view?.now_playing ?? null);
  // Playlists holding the track first, then the rest by name (as the Playlists page's "first").
  const playlists = $derived(
    [...(live.view?.playlists ?? [])].sort((a, b) => Number(b.member) - Number(a.member) || a.name.localeCompare(b.name)),
  );

  let mode = $state<'menu' | 'new'>('menu');
  let working = $state(false);
  let nameField = $state('');

  async function queue(how: 'next' | 'last') {
    if (!np) return;
    working = true;
    try {
      await queueTrack(np.id, how, np.source);
      toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      onclose();
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
    if (!name) return;
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
  <Popover {anchor} label={np.name} {onclose}>
    <div class="head">
      {np.name}
      <span>{[np.artist, np.album].filter(Boolean).join(' · ')}</span>
    </div>
    {#if mode === 'menu'}
      <div class="items" role="menu">
        <button role="menuitem" disabled={working} onclick={() => queue('next')}>Play next</button>
        <button role="menuitem" disabled={working} onclick={() => queue('last')}>Add to queue</button>
        {#if np.favorite !== null}
          <button role="menuitem" disabled={live.pending} onclick={toggleFavorite}
            >{favorite() ? 'Remove from favourites' : 'Add to favourites'}</button
          >
        {/if}
        {#if np.addable && playlists.length}
          <hr />
          <div class="lists" role="group" aria-label="Playlists">
            {#each playlists as p (p.id)}
              <button role="menuitemcheckbox" aria-checked={p.member} class:member={p.member} disabled={live.pending} onclick={() => toggle(p.id, p.member)}
                ><span class="mark" aria-hidden="true">{p.member ? '✓' : '+'}</span>{p.name}</button
              >
            {/each}
          </div>
        {/if}
        <hr />
        <button role="menuitem" onclick={() => (mode = 'new')}>New playlist from the queue</button>
      </div>
    {:else}
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          makePlaylist();
        }}
      >
        <input type="text" aria-label="Name of the new playlist" placeholder="Name" maxlength="100" bind:value={nameField} />
        <div class="row">
          <button type="submit" class="primary" disabled={working || !nameField.trim()} aria-busy={working}>Make playlist</button>
          <button type="button" onclick={onclose}>Cancel</button>
        </div>
      </form>
    {/if}
  </Popover>
{/if}

<style>
  .head {
    padding: 8px 10px 6px;
    font-weight: 650;
    overflow-wrap: anywhere;
  }

  .head span {
    display: block;
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 400;
  }

  .items {
    display: grid;
    min-width: 240px;
  }

  .items button {
    min-height: 44px;
    padding: 0 10px;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 8px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .items hr {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .items button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .items button:hover,
  .items button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  /* The playlists: a scrolling list inside the menu, the ones holding the track marked. */
  .lists {
    display: grid;
    max-height: min(40vh, 320px);
    overflow-y: auto;
  }

  .lists button {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
  }

  .lists .mark {
    width: 1em;
    color: var(--muted);
    text-align: center;
  }

  .lists button.member .mark {
    color: var(--accent);
  }

  .form {
    display: grid;
    gap: 10px;
    padding: 6px;
  }

  .form input {
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .row button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .row button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
