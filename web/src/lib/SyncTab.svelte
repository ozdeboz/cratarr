<script lang="ts">
  // The panel's Sync tab (decisions 0014, 0025): every target, linked or not, for the selected playlists, and how each
  // link is kept. With several playlists a target linked for only some shows a dash and "3 of 5"; ticking links all of
  // them, unticking takes all their links off; a direction or conflict rule chosen applies to all ("Mixed" until then).
  // Each change is made at once.
  import { getTargets, linkPlaylist, unlinkPlaylist, type Conflict, type Direction, type Link, type TargetChoice } from './api';
  import { refresh } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
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
  async function set(t: TargetChoice, on: boolean, direction?: Direction, conflict?: Conflict) {
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
        const d = direction ?? l?.direction ?? 'push';
        const c = conflict ?? l?.conflict ?? 'newest';
        if (l && l.direction === d && l.conflict === c) continue;
        changed[key(id, t.target)] = { target: t.target, label: t.label, direction: d, conflict: c, status: l?.status ?? null };
        touched.push(id);
        await linkPlaylist(id, t.target, d, c);
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
      {@const direction = common(links.map((l) => l.direction))}
      {@const conflict = common(links.map((l) => l.conflict))}
      {@const errors = ids.flatMap((id) => {
        const e = linkOf(id, t.target)?.status?.error;
        return e ? [ids.length > 1 ? `${id}: ${e}` : e] : [];
      })}
      <div class="target">
        <label class="on"
          ><input
            type="checkbox"
            checked={all}
            indeterminate={some}
            disabled={working}
            onchange={(e) => set(t, e.currentTarget.checked)}
          />{t.label}{#if some}<span class="n">{links.length} of {ids.length}</span>{/if}</label
        >
        {#if links.length && !t.push_only}
          <select aria-label="Direction" value={direction ?? ''} disabled={working} onchange={(e) => set(t, true, e.currentTarget.value as Direction)}>
            {#if direction === null}<option value="" disabled>Mixed</option>{/if}
            <option value="push">Push</option>
            <option value="two_way">Two-way</option>
          </select>
          {#if direction === 'two_way'}
            <select
              aria-label="When both changed"
              value={conflict ?? ''}
              disabled={working}
              onchange={(e) => set(t, true, 'two_way', e.currentTarget.value as Conflict)}
            >
              {#if conflict === null}<option value="" disabled>Mixed</option>{/if}
              <option value="newest">Newest wins</option>
              <option value="playlist">Cratarr wins</option>
              <option value="target">{t.label} wins</option>
            </select>
          {/if}
        {/if}
        {#each errors as e (e)}<p class="error">{e}</p>{/each}
      </div>
    {/each}
  {/if}
</div>

<style>
  .tab {
    padding: 4px 16px 0;
  }

  .empty {
    color: var(--muted);
  }

  /* One line per target, its settings beside it once linked. */
  .target {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    min-height: 44px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }

  .on {
    display: flex;
    flex: 1 1 160px;
    align-items: center;
    gap: 10px;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  input[type='checkbox'] {
    flex: none;
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
  }

  .n {
    margin-left: auto;
    color: var(--muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
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
    flex-basis: 100%;
    margin: 0;
    color: var(--danger);
    font-size: 0.8rem;
  }
</style>
