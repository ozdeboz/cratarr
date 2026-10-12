<script lang="ts">
  import Playlists from './Playlists.svelte';
  import Repairs from './Repairs.svelte';
  import Manage from './Manage.svelte';

  type Page = 'playlists' | 'repairs' | 'manage';
  const fromHash = (): Page => (location.hash === '#/repairs' ? 'repairs' : location.hash === '#/manage' ? 'manage' : 'playlists');
  let page = $state<Page>(fromHash());

  $effect(() => {
    const on = () => (page = fromHash());
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });
</script>

<nav aria-label="Pages">
  <a href="#/" aria-current={page === 'playlists' ? 'page' : undefined}>Playlists</a>
  <a href="#/repairs" aria-current={page === 'repairs' ? 'page' : undefined}>Repairs</a>
  <a href="#/manage" aria-current={page === 'manage' ? 'page' : undefined}>Manage</a>
</nav>

{#if page === 'repairs'}
  <Repairs />
{:else if page === 'manage'}
  <Manage />
{:else}
  <Playlists />
{/if}

<style>
  nav {
    display: flex;
    gap: 4px;
    padding: 8px clamp(12px, 2vw, 32px) 0;
  }

  a {
    padding: 6px 12px;
    border-radius: 8px;
    color: var(--muted);
    text-decoration: none;
    font-size: 0.9rem;
  }

  a[aria-current='page'] {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-weight: 600;
  }
</style>
