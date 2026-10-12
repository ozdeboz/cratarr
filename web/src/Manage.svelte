<script lang="ts">
  import ManagePlaylists from './ManagePlaylists.svelte';
  import Repairs from './Repairs.svelte';

  const fromHash = () => (location.hash === '#/manage/repairs' || location.hash === '#/repairs' ? 'repairs' : 'playlists');
  let tab = $state<'playlists' | 'repairs'>(fromHash());

  $effect(() => {
    const on = () => (tab = fromHash());
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });
</script>

<main>
  <h1>Manage</h1>
  <div class="tabs" role="tablist" aria-label="Manage">
    <a role="tab" href="#/manage" aria-selected={tab === 'playlists'}>Playlists</a>
    <a role="tab" href="#/manage/repairs" aria-selected={tab === 'repairs'}>Repairs</a>
  </div>

  {#if tab === 'repairs'}
    <Repairs />
  {:else}
    <ManagePlaylists />
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 8px clamp(12px, 2vw, 32px) 48px;
  }

  h1 {
    margin: 16px 0 12px;
    font-size: 1.25rem;
  }

  .tabs {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    margin-bottom: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .tabs a {
    display: flex;
    align-items: center;
    min-height: 36px;
    padding: 0 16px;
    border-radius: 8px;
    color: var(--muted);
    text-decoration: none;
    font-size: 0.9rem;
  }

  .tabs a[aria-selected='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
</style>
