<script lang="ts">
  // New playlist from the queue (decision 0025; the owner put it at the queue sheet's top, 2026-10-16): what plays and
  // what comes next as a new playlist, named here. A queue that plays a track more than once asks first: Keep repeats,
  // or each track once (decision 0025, amended 2026-10-16). It comes from the top, where the keyboard leaves it in view.
  import { playlistFromQueue } from './api';
  import { followCreate, followDelete } from './follow.svelte';
  import { refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { port } from './player/port.svelte';
  import { playing } from './remote.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const np = $derived(playing());
  const held = $derived(port().queue());
  // The queue from the playing track on: this device's own, or what the server's player said.
  const ids = $derived(
    held ? held.tracks.slice(Math.max(0, held.index)).map((x) => x.id) : np ? [np.id, ...np.upcoming.map((u) => u.id)] : [],
  );
  const repeated = $derived(new Set(ids).size < ids.length);

  let step = $state<'name' | 'repeats'>('name');
  let name = $state('');
  let working = $state(false);

  async function make(repeats = false) {
    const n = name.trim();
    if (!n || working) return;
    working = true;
    try {
      const own = held ? held.tracks.slice(Math.max(0, held.index)).map((t) => t.id) : undefined;
      const made = await playlistFromQueue(n, np?.source, own, repeats);
      const left = made.left_out ? `, ${made.left_out} not in the library left out` : '';
      withUndo(`Made ${made.playlist}: ${made.added} ${made.added === 1 ? 'track' : 'tracks'}${left}.`, [made.undo], {
        after: (undone) => (undone ? followDelete(made.playlist) : followCreate(made.playlist)),
      });
      onclose();
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
    } finally {
      working = false;
    }
  }
</script>

<MenuSheet
  label="New playlist from the queue"
  {onclose}
  images={null}
  icon="intoBox"
  title="New playlist from the queue"
  onback={step === 'repeats' ? () => (step = 'name') : null}
  {step}
>
  {#if step === 'repeats'}
    <div class="heading">The queue plays some tracks more than once</div>
    <div role="group" aria-label="Repeated tracks">
      <MenuEntry icon="listAdd" label="Keep repeats" disabled={working} onclick={() => make(true)} />
      <MenuEntry icon="check" label="Each track once" disabled={working} onclick={() => make(false)} />
    </div>
  {:else}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        if (repeated) step = 'repeats';
        else make();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" aria-label="Name of the new playlist" placeholder="Name" maxlength="100" autofocus bind:value={name} />
      <button type="submit" class="primary" disabled={working || !name.trim() || !ids.length} aria-busy={working}>Make playlist</button>
    </form>
  {/if}
</MenuSheet>

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
