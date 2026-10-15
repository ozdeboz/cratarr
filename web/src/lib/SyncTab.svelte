<script lang="ts">
  // The panel's Sync tab (decisions 0014, 0025): every target, linked or not, for the selected playlists, and how each
  // link is kept. With several playlists a target linked for only some shows a dash and "3 of 5"; ticking links all of
  // them, unticking takes all their links off; a conflict rule chosen applies to all ("Mixed" until then). Every link
  // is kept both ways, except Shared: a copy the server cannot edit. Each change is made at once.
  import { getTargets, linkPlaylist, unlinkPlaylist, type Conflict, type Link, type TargetChoice } from './api';
  import { refresh } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';

  let { ids }: { ids: string[] } = $props();

  let targets = $state<TargetChoice[] | null>(null);
  $effect(() => {
    getTargets().then(
      (t) => (targets = t),
      (e) => toast.fail(e, 'Could not list the targets'),
    );
  });

  // The links as the server last said, with what was just changed laid over them until it says again.
  let changed = $state<Record<string, Link | null>>({});
  const key = (id: string, target: string) => `${id}\n${target}`;
  const linkOf = (id: string, target: string): Link | null => {
    const k = key(id, target);
    if (k in changed) return changed[k];
    return shownPlaylists().find((p) => p.id === id)?.links.find((l) => l.target === target) ?? null;
  };

  let working = $state(false);
  async function set(t: TargetChoice, on: boolean, conflict?: Conflict) {
    if (working) return;
    working = true;
    const touched: string[] = [];
    try {
      for (const id of ids) {
        const l = linkOf(id, t.target);
        if (!on) {
          if (!l) continue;
          changed[key(id, t.target)] = null;
          touched.push(id);
          await unlinkPlaylist(id, t.target);
          continue;
        }
        const c = conflict ?? l?.conflict ?? 'newest';
        if (l && l.conflict === c) continue;
        changed[key(id, t.target)] = { target: t.target, label: t.label, conflict: c, status: l?.status ?? null };
        touched.push(id);
        await linkPlaylist(id, t.target, c);
      }
    } catch (e) {
      for (const id of touched) delete changed[key(id, t.target)];
      toast.fail(e, 'Could not change the link');
    } finally {
      working = false;
      await refresh();
      changed = {};
    }
  }

  /** One value when every linked playlist has it, else null ("Mixed"). */
  function common<T>(values: T[]): T | null {
    return values.length && values.every((v) => v === values[0]) ? values[0] : null;
  }
</script>

<div class="tab">
  {#if !targets}
    <p class="empty">Loading…</p>
  {:else}
    {#each targets as t (t.target)}
      {@const links = ids.map((id) => linkOf(id, t.target)).filter((l) => !!l)}
      {@const all = links.length === ids.length}
      {@const some = links.length > 0 && !all}
      {@const conflict = common(links.map((l) => l.conflict))}
      {@const errors = ids.flatMap((id) => {
        const e = linkOf(id, t.target)?.status?.error;
        return e ? [ids.length > 1 ? `${id}: ${e}` : e] : [];
      })}
      <!-- A large row: ticking its box links it; once linked, how it is kept is its action (under it when narrow). -->
      <Row size="large" as="label" title={t.label} count={some ? `${links.length} of ${ids.length}` : undefined} wrap>
        {#snippet lead()}
          <input type="checkbox" checked={all} indeterminate={some} disabled={working} onchange={(e) => set(t, e.currentTarget.checked)} />
        {/snippet}
        {#snippet sub()}{#if errors.length}<span class="error">{errors.join(' · ')}</span>{/if}{/snippet}
        {#snippet action()}
          {#if links.length && !t.copy_only}
            <select aria-label="When both changed" value={conflict ?? ''} disabled={working} onchange={(e) => set(t, true, e.currentTarget.value as Conflict)}>
              {#if conflict === null}<option value="" disabled>Mixed</option>{/if}
              <option value="newest">Newest wins</option>
              <option value="playlist">Cratarr wins</option>
              <option value="target">{t.label} wins</option>
            </select>
          {/if}
        {/snippet}
      </Row>
    {/each}
  {/if}
</div>

<style>
  /* Large rows (lib/Row.svelte); what is the tab's own is the controls. */
  .tab {
    padding: 0 8px;
  }

  .empty {
    margin: 12px;
    color: var(--muted);
  }

  input[type='checkbox'] {
    flex: none;
    width: 18px;
    height: 18px;
    margin: 0 4px 0 12px;
    accent-color: var(--accent);
  }

  select {
    min-height: 34px;
    padding: 0 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
  }

  .error {
    color: var(--danger);
    white-space: normal;
  }
</style>
