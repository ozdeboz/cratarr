<script lang="ts">
  import { ApiError, createPlaylist, deletePlaylist, getState, renamePlaylist, restorePlaylist, type Playlist } from './lib/api';

  let playlists = $state<Playlist[]>([]);
  let error = $state<string | null>(null);
  let notice = $state<{ text: string; restore?: string } | null>(null);
  let newName = $state('');
  let renaming = $state<string | null>(null);
  let renameTo = $state('');
  let deleting = $state<string | null>(null);
  let busy = $state(false);

  async function load() {
    try {
      playlists = (await getState()).playlists;
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Unexpected error';
    }
  }

  $effect(() => {
    load();
  });

  async function run(action: () => Promise<unknown>, done: { text: string; restore?: string }) {
    if (busy) return;
    busy = true;
    try {
      await action();
      notice = done;
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Something went wrong' };
    } finally {
      busy = false;
      renaming = deleting = null;
      await load();
    }
  }

  const create = () => {
    const name = newName.trim();
    if (!name) return;
    run(() => createPlaylist(name), { text: `Created “${name}”.` }).then(() => (newName = ''));
  };
</script>

<main>
  <h1>Manage</h1>
  <p class="lede">Create, rename and delete playlists. A deleted playlist is kept in the backup folder and can be restored.</p>

  {#if error}<div class="banner error" role="alert">{error}</div>{/if}
  {#if notice}
    <div class="banner" role="status">
      {notice.text}
      {#if notice.restore}<button class="link" disabled={busy} onclick={() => run(() => restorePlaylist(notice!.restore!), { text: 'Restored.' })}>Undo</button>{/if}
    </div>
  {/if}

  <form class="new" onsubmit={(e) => { e.preventDefault(); create(); }}>
    <input type="text" placeholder="New playlist name" aria-label="New playlist name" bind:value={newName} maxlength="100" />
    <button class="primary" type="submit" disabled={busy || !newName.trim()}>Create</button>
  </form>

  <ul class="rows">
    {#each playlists as p (p.id)}
      <li>
        {#if renaming === p.id}
          <form class="inline" onsubmit={(e) => { e.preventDefault(); run(() => renamePlaylist(p.id, renameTo.trim()), { text: `Renamed to “${renameTo.trim()}”.` }); }}>
            <input type="text" aria-label="New name" bind:value={renameTo} maxlength="100" />
            <button class="primary" type="submit" disabled={busy || !renameTo.trim() || renameTo.trim() === p.id}>Save</button>
            <button type="button" onclick={() => (renaming = null)}>Cancel</button>
          </form>
        {:else}
          <span class="name">{p.name}</span>
          <span class="n">{p.count}</span>
          <span class="actions">
            {#if deleting === p.id}
              <button class="danger" disabled={busy} onclick={() => run(() => deletePlaylist(p.id), { text: `Deleted “${p.name}”.`, restore: p.id })}>Delete “{p.name}”?</button>
              <button onclick={() => (deleting = null)}>Cancel</button>
            {:else}
              <button onclick={() => { renaming = p.id; renameTo = p.id; }}>Rename</button>
              <button onclick={() => (deleting = p.id)}>Delete</button>
            {/if}
          </span>
        {/if}
      </li>
    {/each}
  </ul>
</main>

<style>
  main {
    max-width: 900px;
    padding: 8px clamp(12px, 2vw, 32px) 48px;
  }

  h1 {
    margin: 16px 0 4px;
    font-size: 1.25rem;
  }

  .lede {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .banner {
    margin: 12px 0;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 0.9rem;
  }

  .banner.error {
    border-color: var(--danger);
    color: var(--danger);
  }

  input {
    flex: 1 1 200px;
    min-height: 40px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button.danger {
    border-color: var(--danger);
    color: var(--danger);
  }

  button.link {
    min-height: 0;
    margin-left: 10px;
    padding: 0;
    background: none;
    border: 0;
    color: var(--accent);
    text-decoration: underline;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .new,
  .inline {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    width: 100%;
    margin: 12px 0;
  }

  .inline {
    margin: 0;
  }

  .rows {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 56px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .n {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
</style>
