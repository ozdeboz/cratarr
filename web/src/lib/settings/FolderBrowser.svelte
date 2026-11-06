<script lang="ts">
  // Add folder (decision 0044): a browser of what Cratarr can see, its mounts at the top, each folder with how much
  // audio is in it; the music folders Cratarr found by itself come first there, as suggestions. Choosing one adds it.
  import { browseFolders, suggestedFolders, type FolderEntry, type FolderListing } from '../api';
  import Asking from '../Asking.svelte';
  import { keepWhile } from '../keepWhile.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let { chosen, choose = 'Add this folder', onadd, onclose }: { chosen: string[]; choose?: string; onadd: (path: string) => void; onclose: () => void } = $props();

  let at = $state<FolderListing | null>(null);
  let error = $state<string | null>(null);
  let found = $state<string[]>([]);

  /** The folders asked for so far: only the last one's answer counts, a tap on the way taking over from the one before. */
  let asked = 0;
  /** The one waited for, 0 with none. */
  let waiting = $state(0);
  // The folder shown stays while the next one opens; a slow one finds the loader in its place (lib/loader, the one rule).
  const wait = keepWhile(
    () => null,
    () => waiting !== 0,
    () => waiting,
  );

  async function open(path: string | null) {
    const mine = (waiting = ++asked);
    error = null;
    try {
      const listing = await browseFolders(path);
      if (mine === asked) at = listing;
    } catch (e) {
      if (mine === asked) error = e instanceof Error ? e.message : String(e);
    }
    if (mine === asked) waiting = 0;
  }
  open(null);
  suggestedFolders().then(
    (r) => (found = r.folders),
    () => (found = []),
  );

  const isChosen = (p: string) => chosen.includes(p);
  const name = (p: string) => p.split('/').filter(Boolean).pop() ?? p;
  const tracks = (e: FolderEntry) => (e.audio === 0 ? null : `${e.audio}${e.more ? '+' : ''} ${e.audio === 1 && !e.more ? 'track' : 'tracks'}`);
  const shownFound = $derived(found.filter((p) => !isChosen(p)));

  function add(path: string) {
    onadd(path);
    onclose();
  }
</script>

<MenuSheet
  label="Add folder"
  icon="library"
  title={at?.path ? name(at.path) : 'Add folder'}
  chips={at?.path ? [at.path] : []}
  onback={at?.path ? () => open(at?.parent ?? null) : null}
  {onclose}
>
  <div role="menu">
    {#if wait.loader || (!at && !error)}
      <Asking now={wait.loader} />
    {:else}
      {#if !at?.path}
        {#each shownFound as p (p)}
          <MenuEntry icon="circlePlus" label={`Add ${p}`} onclick={() => add(p)} />
        {/each}
      {/if}
      <!-- A row, not the sheet's actions: those give way to the back row, which every folder but the top one has. -->
      {#if at?.path && !isChosen(at.path)}
        {@const here = at.path}
        <MenuEntry icon="circlePlus" label={choose} onclick={() => add(here)} />
      {/if}
      {#each at?.folders ?? [] as e (e.path)}
        <MenuEntry icon="library" label={[e.name, isChosen(e.path) ? 'added' : tracks(e), e.writable ? null : 'read only'].filter(Boolean).join(' · ')} onclick={() => open(e.path)} />
      {:else}
        {#if at}<p class="hint">No folders here.</p>{/if}
      {/each}
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</MenuSheet>

<style>
  .hint {
    margin: 12px 20px;
    color: var(--muted);
  }

  .error {
    margin: 12px 20px;
    color: var(--danger-ink);
  }
</style>
