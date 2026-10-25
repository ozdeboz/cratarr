<script lang="ts">
  // "Not for …" (the owner, 2026-10-23): a ranked crate's fit, its other side, correctable by hand. What it learned
  // from what was turned down for it (a trait's ✕: "that's not why", it stops counting; a dismissed one comes back
  // with a tap), one's own rules (strict: the crate builder's condition cards, saved as they change), and what was
  // turned down (each put back, with Undo as excluding has).
  import { untrack } from 'svelte';
  import { coverUrl, crateLeftOut, getCrateAxes, getCrateFit, putBackInCrate, setCrateFit, traitLabel, type Axis, type CrateFit, type PlaylistRow, albumCoverUrl, type ExcludedAlbum } from './api';
  import ConditionList from './ConditionList.svelte';
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs, type FilterCondition } from './prefs.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  let { crate, name, onchange, onclose }: { crate: string; name: string; onchange: () => void; onclose: () => void } = $props();

  let fit = $state<CrateFit | null>(null);
  let rules = $state<FilterCondition[]>([]);
  let axes = $state<Axis[] | null>(null);
  let down = $state<{ source: string; rows: PlaylistRow[]; albums: ExcludedAlbum[] } | null>(null);
  let busy = $state(false);
  /** The rules as last kept. */
  let saving = '';

  async function load() {
    try {
      const [f, l] = await Promise.all([getCrateFit(crate), crateLeftOut(crate, prefs.source || undefined)]);
      fit = f;
      rules = f.rules;
      saving = JSON.stringify(f.rules);
      down = { source: l.source, rows: l.rows, albums: l.albums ?? [] };
    } catch (e) {
      toast.fail(e, "The crate's fit couldn't be read");
    }
  }
  $effect(() => {
    void crate;
    untrack(load);
  });
  $effect(() => {
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      () => (axes ??= []),
    );
  });

  const dismissed = $derived((fit?.learned ?? []).filter((l) => l.dismissed).map((l) => l.key));
  async function save(next: { rules?: FilterCondition[]; dismissed?: string[] }) {
    try {
      await setCrateFit(crate, { rules: next.rules ?? rules, dismissed: next.dismissed ?? dismissed });
      onchange();
    } catch (e) {
      toast.fail(e, "The change wasn't kept");
    }
  }
  async function dismiss(key: string, out: boolean) {
    const before = dismissed;
    const after = out ? [...before, key] : before.filter((k) => k !== key);
    await save({ dismissed: after });
    await load();
  }
  // The rules as the cards leave them, kept once they rest (300 ms).
  $effect(() => {
    const k = JSON.stringify(rules);
    if (k === saving) return;
    const t = setTimeout(() => {
      saving = k;
      void save({ rules: JSON.parse(k) });
    }, 300);
    return () => clearTimeout(t);
  });

  /** An album left out of the crate (Hide album, beside its Avoiding line): back in. */
  async function albumBack(a: ExcludedAlbum) {
    const d = down;
    if (!d) return;
    busy = true;
    try {
      const { undo } = await putBackInCrate(crate, d.source, { albums: [a.id] });
      withUndo(`${a.title ?? 'The album'} is suggested for ${name} again.`, [undo], { after: onchange });
      onchange();
      await load();
    } catch (e) {
      toast.fail(e, "It wasn't put back");
    } finally {
      busy = false;
    }
  }
  async function putBack(r: PlaylistRow) {
    const d = down;
    if (!d || !r.id) return;
    busy = true;
    try {
      const { undo } = await putBackInCrate(crate, d.source, { ids: [r.id] });
      withUndo(`Suggested for ${name} again.`, [undo], { after: onchange });
      onchange();
      await load();
    } catch (e) {
      toast.fail(e, "It wasn't put back");
    } finally {
      busy = false;
    }
  }
</script>

<MenuSheet label="Not for {name}" icon="sliders" title="Not for {name}" {onclose}>
  {#if fit}
    <h3>Learned from {fit.turned_down} turned down</h3>
    {#if fit.learned.length}
      <p class="chips">
        {#each fit.learned as l (l.key)}
          {#if l.dismissed}
            <button class="chip off" aria-label="Count {traitLabel(l.trait)} again" onclick={() => dismiss(l.key, false)}>{traitLabel(l.trait)}<Icon name="undo" size={16} /></button>
          {:else}
            <button class="chip" aria-label="{traitLabel(l.trait)} is not why" onclick={() => dismiss(l.key, true)}>{traitLabel(l.trait)}<Icon name="close" size={16} /></button>
          {/if}
        {/each}
      </p>
    {/if}
    <h3>Your rules</h3>
    <div class="rules"><ConditionList bind:conditions={rules} {axes} /></div>
    {#if down?.albums.length}
      <h3>Albums left out</h3>
      <ol class="rows" aria-busy={busy}>
        {#each down.albums as a (a.id)}
          <li>
            <Row size="large" title={a.title ?? 'An album no longer in the library'} sub={a.artist ?? ''} off={!a.title}>
              {#snippet lead()}<TrackCover src={a.title ? albumCoverUrl(down!.source, a.id) : null} />{/snippet}
              {#snippet action()}
                <button class="back" disabled={busy} onclick={() => albumBack(a)}>Put back</button>
              {/snippet}
            </Row>
          </li>
        {/each}
      </ol>
    {/if}
    {#if down?.rows.length}
      <h3>Turned down</h3>
      <ol class="rows" aria-busy={busy}>
        {#each down.rows as r, i (i)}
          <li>
            <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || r.missing}>
              {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: down!.source, id: r.id }) : null} />{/snippet}
              {#snippet action()}
                <button class="back" disabled={busy || !r.id} onclick={() => putBack(r)}>Put back</button>
              {/snippet}
            </Row>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</MenuSheet>

<style>
  h3 {
    margin: 0;
    padding: 16px 24px 8px;
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0 24px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .chip :global(svg) {
    color: var(--muted);
  }

  .chip.off {
    color: var(--muted);
    text-decoration: line-through;
  }

  .rules {
    padding: 0 12px;
  }

  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    margin: 0;
    padding: 0 8px 8px;
    list-style: none;
  }

  .back {
    padding: 8px 10px;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .back:disabled {
    opacity: 0.4;
  }
</style>
