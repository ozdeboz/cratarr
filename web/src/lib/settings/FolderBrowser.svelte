<script lang="ts">
  // Add folder (decision 0044): a browser of what Cratarr can see, its mounts at the top, each folder with how much
  // audio is in it; the music folders Cratarr found by itself come first there, as suggestions. Choosing one adds it.
  import { browseFolders, suggestedFolders, type FolderEntry, type FolderListing } from '../api';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let { chosen, onadd, onclose }: { chosen: string[]; onadd: (path: string) => void; onclose: () => void } = $props();

  let at = $state<FolderListing | null>(null);
  let error = $state<string | null>(null);
  let found = $state<string[]>([]);

  async function open(path: string | null) {
    error = null;
    try {
      at = await browseFolders(path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
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
  actions={at?.path && !isChosen(at.path) ? [{ icon: 'plus', label: 'Add this folder', onclick: () => at?.path && add(at.path) }] : []}
  onback={at?.path ? () => open(at?.parent ?? null) : null}
  {onclose}
>
  <div role="menu">
    {#if !at?.path}
      {#each shownFound as p (p)}
        <MenuEntry icon="circlePlus" label={`Add ${p}`} onclick={() => add(p)} />
      {/each}
    {/if}
    {#each at?.folders ?? [] as e (e.path)}
      <MenuEntry icon="library" label={[e.name, isChosen(e.path) ? 'added' : tracks(e), e.writable ? null : 'read only'].filter(Boolean).join(' · ')} onclick={() => open(e.path)} />
    {:else}
      {#if at}<p class="hint">No folders here.</p>{/if}
    {/each}
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
