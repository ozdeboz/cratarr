<script lang="ts">
  // A crate's tracks taken out by hand (decision 0024, amended 2026-10-19), from the "N left out" line under its name in
  // the panel (any crate's: one's own, Unlisted, a genre's): each can be put back, or all at once, with Undo as taking
  // out has. No notice besides; an error in one line.
  import { coverUrl, crateLeftOut, putBackInCrate, type PlaylistRow } from './api';
  import { crateName, loadCrates } from './crates.svelte';
  import Icon from './Icon.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import PageLoading from './PageLoading.svelte';
  import { prefs } from './prefs.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  /** `onchange`: tracks went back in (the panel reads the crate's rows again). */
  let { crate, onchange, onclose }: { crate: string; onchange: () => void; onclose: () => void } = $props();

  let list = $state<{ source: string; rows: PlaylistRow[] } | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function load() {
    try {
      list = await crateLeftOut(crate, prefs.source || undefined);
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void crate;
    load();
  });

  async function putBack(rows: PlaylistRow[]) {
    const l = list;
    const ids = rows.flatMap((r) => (r.id ? [r.id] : []));
    if (!l || !ids.length) return;
    busy = true;
    try {
      const { undo } = await putBackInCrate(crate, l.source, ids);
      withUndo(`Put back in ${crateName(crate)}.`, [undo], {
        after: () => {
          loadCrates();
          onchange();
        },
      });
      await loadCrates();
      onchange();
      await load();
      if (!list?.rows.length) onclose();
    } catch (e) {
      toast.fail(e, "It wasn't put back");
    } finally {
      busy = false;
    }
  }
</script>

<MenuSheet label="Left out" icon="crateOut" title="Left out" {onclose}>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !list}
    <PageLoading />
  {:else}
    {@const l = list}
    {#if l.rows.length > 1}<MenuEntry icon="undo" label="Put all back" disabled={busy} onclick={() => putBack(l.rows)} />{/if}
    <ol class="rows" aria-busy={busy}>
      {#each l.rows as r, i (i)}
        <li>
          <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || r.missing}>
            {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: l.source, id: r.id }) : null} />{/snippet}
            {#snippet action()}
              <button class="back" aria-label={`Put ${r.name ?? r.file} back`} title="Put back" disabled={busy || !r.id} onclick={() => putBack([r])}
                ><Icon name="undo" size={22} /></button
              >
            {/snippet}
          </Row>
        </li>
      {/each}
    </ol>
  {/if}
</MenuSheet>

<style>
  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    margin: 0;
    padding: 0 8px 8px;
    overflow-y: auto;
    list-style: none;
  }

  .back {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .back:hover,
  .back:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .back:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .error {
    margin: 0;
    padding: 0 24px 10px;
    color: var(--danger);
    font-size: 14px;
  }
</style>
