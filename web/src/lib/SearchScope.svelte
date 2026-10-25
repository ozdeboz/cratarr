<script lang="ts">
  // Two tabs under the search (the owner, 2026-10-25, mockup 3 of 8): the page's or the panel's own results, or
  // everywhere's (lib/GlobalResults), each with its count; the page's first, everywhere only on a tap, so the page's
  // own search is never mixed with anything. Shown while a search is on. The slim track (lib/Seg).
  import { globalAsked, globalGroups, globalSearch } from './globalSearch.svelte';
  import { pageView } from './pageView.svelte';
  import { words } from './search';
  import Seg from './Seg.svelte';

  let {
    label,
    count = null,
    except = [],
  }: {
    /** The page's or the panel's name. */
    label: string;
    /** How many of its own the search leaves, where it says. */
    count?: number | null;
    /** The kinds it shows itself (lib/GlobalResults). */
    except?: string[];
  } = $props();

  const all = $derived(globalGroups(except).reduce((n, g) => n + g.total, 0));
  const options = $derived([
    { value: 'page' as const, label: count === null ? label : `${label} ${count}` },
    { value: 'everywhere' as const, label: globalAsked() || all ? `Everywhere ${all}` : 'Everywhere' },
  ]);
</script>

{#if words(pageView.query).length}
  <div class="search-scope">
    <Seg label="Search in" {options} bind:value={globalSearch.scope} />
  </div>
{/if}

<style>
  .search-scope {
    display: flex;
    justify-content: center;
    margin: 2px 0 12px;
  }
</style>
