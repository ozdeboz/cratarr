<script lang="ts">
  // A slow page's (or a refused path's, a throttled client's) recent events, Health's third level (the owner,
  // 2026-10-24): read from the diagnostics log (decision 0045), the ones of this kind that match the item, newest first:
  // when, how often, how slow, and who asked or what.
  import { getLog, type LogRow } from './api';

  /** The log's area and event, the field the item is grouped by (`by`) and its value (the item). */
  let { area, what, by, value }: { area: string; what: string; by: string; value: string } = $props();

  let rows = $state<LogRow[] | null>(null);
  $effect(() => {
    const [ar, wh, b, v] = [area, what, by, value];
    let gone = false;
    rows = null;
    getLog({ area: ar, limit: 500 }).then(
      (r) => !gone && (rows = r.rows.filter((x) => x.what === wh && String(x.fields[b] ?? '') === v).slice(0, 50)),
      () => !gone && (rows = []),
    );
    return () => (gone = true);
  });
  const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  /** What else a row says: how slow, and the other of client or path. */
  const rest = (x: LogRow) =>
    [
      typeof x.fields.ms === 'number' ? `${x.fields.ms} ms` : null,
      by !== 'client' && x.fields.client ? `from ${x.fields.client}` : null,
      by !== 'path' && x.fields.path ? String(x.fields.path) : null,
    ]
      .filter(Boolean)
      .join(' · ');
</script>

{#if rows === null}
  <p class="muted">…</p>
{:else if !rows.length}
  <p class="muted">None kept any more.</p>
{:else}
  <ol class="events">
    {#each rows as x (x.id)}
      <li>
        <span class="at">{time(x.last)}{x.count > 1 ? ` · ${x.count}×` : ''}</span>
        <span class="what">{rest(x)}</span>
      </li>
    {/each}
  </ol>
{/if}

<style>
  .muted {
    margin: 0;
    color: var(--muted);
  }

  .events {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .events li {
    display: grid;
    gap: 2px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .at {
    font-variant-numeric: tabular-nums;
  }

  .what {
    color: var(--muted);
    font-size: 0.9rem;
    overflow-wrap: anywhere;
  }
</style>
