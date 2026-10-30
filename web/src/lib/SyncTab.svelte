<script lang="ts">
  // The panel's Sync tab (decisions 0014, 0025): whether the selected playlists are shared with everyone, and their
  // copy in each account, with any problem it has. Your account has every playlist and follows by itself; sharing puts
  // a playlist in every account. With several playlists: "3 of 5" where only some are; ticking shares all of them.
  import { batchLink, missingCall } from './api';
  import { trackSteps } from './progress.svelte';
  import { linkPlaylist, unlinkPlaylist, type Link } from './api';
  import { refresh } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';

  let { ids }: { ids: string[] } = $props();

  const SHARED = 'shared';
  const linksOf = (id: string): Link[] => shownPlaylists().find((p) => p.id === id)?.links ?? [];

  // Shared as the server last said, with what was just changed laid over it until it says again.
  let changed = $state<Record<string, boolean>>({});
  const isShared = (id: string) => (id in changed ? changed[id] : linksOf(id).some((l) => l.target === SHARED));
  const sharedCount = $derived(ids.filter(isShared).length);

  let working = $state(false);
  async function share(on: boolean) {
    if (working) return;
    working = true;
    const touched: string[] = [];
    try {
      const changing = ids.filter((id) => isShared(id) !== on);
      // One call where the server has it (0.9.4); else one by one.
      try {
        for (const id of changing) ((changed[id] = on), touched.push(id));
        const r = await batchLink(changing, on, SHARED);
        if (r.failed.length) throw new Error(r.failed[0].error);
        return;
      } catch (e) {
        if (!missingCall(e)) throw e;
        touched.length = 0;
      }
      await trackSteps(changing.length, async (done) => {
        for (const id of changing) {
          changed[id] = on;
          touched.push(id);
          await (on ? linkPlaylist(id, SHARED) : unlinkPlaylist(id, SHARED));
          done();
        }
      });
    } catch (e) {
      for (const id of touched) delete changed[id];
      toast.fail(e, on ? 'Could not share it' : 'Could not stop sharing it');
    } finally {
      working = false;
      await refresh();
      changed = {};
    }
  }

  // Each account's copies of the selected playlists: how many there are, and their problems.
  const accounts = $derived.by(() => {
    const out = new Map<string, { label: string; count: number; errors: string[] }>();
    for (const id of ids) {
      for (const l of linksOf(id).filter((l) => l.target !== SHARED)) {
        const a = out.get(l.target) ?? { label: l.label, count: 0, errors: [] };
        a.count++;
        if (l.status?.error) a.errors.push(ids.length > 1 ? `${id}: ${l.status.error}` : l.status.error);
        out.set(l.target, a);
      }
    }
    return [...out.values()];
  });
</script>

<div class="tab">
  <Row
    size="large"
    as="label"
    title="Shared with everyone"
    count={sharedCount > 0 && sharedCount < ids.length ? `${sharedCount} of ${ids.length}` : undefined}
    wrap
  >
    {#snippet lead()}
      <input
        type="checkbox"
        checked={sharedCount === ids.length}
        indeterminate={sharedCount > 0 && sharedCount < ids.length}
        disabled={working}
        onchange={(e) => share(e.currentTarget.checked)}
      />
    {/snippet}
  </Row>
  {#each accounts as a (a.label)}
    <Row size="large" title={a.label} count={ids.length > 1 && a.count < ids.length ? `${a.count} of ${ids.length}` : undefined} wrap>
      {#snippet sub()}{#if a.errors.length}<span class="error">{a.errors.join(' · ')}</span>{/if}{/snippet}
    </Row>
  {/each}
</div>

<style>
  /* Large rows (lib/Row.svelte); what is the tab's own is the box. */
  .tab {
    padding: 0 8px;
  }

  input[type='checkbox'] {
    flex: none;
    width: 18px;
    height: 18px;
    margin: 0 4px 0 12px;
    accent-color: var(--accent);
  }

  .error {
    color: var(--danger-ink);
    white-space: normal;
  }
</style>
