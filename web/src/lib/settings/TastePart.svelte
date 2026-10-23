<script lang="ts">
  // Taste (the owner, 2026-10-23: "all the exclusions can be managed" here, and later the weights once AudioMuse
  // has heard everything): what one has said one does not want, in one place. Excluded everywhere (decision 0024,
  // amended: out of every crate and every automatic pick), then each crate that leaves tracks out by hand; each opens
  // its own page here (`thing`: `everywhere`, or `crate:<id>`) with the list to put back (lib/ExcludedList).
  import { crateName, crates, loadCrates } from '../crates.svelte';
  import ExcludedList from '../ExcludedList.svelte';
  import { excluded, loadExcluded } from '../exclusions.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import PageLoading from '../PageLoading.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  $effect(() => {
    loadExcluded();
    void loadCrates();
  });

  const EVERYWHERE = 'everywhere';
  const CRATE = 'crate:';
  /** The crates that leave tracks out by hand, the most first. */
  const leaving = $derived((crates.list ?? []).filter((c) => (c.left_out ?? 0) > 0).sort((a, b) => (b.left_out ?? 0) - (a.left_out ?? 0)));
  const crateAt = $derived(thing?.startsWith(CRATE) ? thing.slice(CRATE.length) : null);
  const known = $derived(crates.list !== null || crates.error !== null);
  nameThing(() => (thing === EVERYWHERE ? 'Excluded everywhere' : crateAt ? crateName(crateAt) : null));
  // A page whose list is all put back, or a thing there is no page for: back to the part.
  const done = () => backFromThing('taste');
  $effect(() => {
    if (thing && thing !== EVERYWHERE && !crateAt) done();
  });

  const count = (n: number) => n.toLocaleString();
</script>

{#if thing === EVERYWHERE}
  <ExcludedList onchange={loadExcluded} onempty={done} />
{:else if crateAt}
  <ExcludedList crate={crateAt} onchange={() => loadCrates()} onempty={done} />
{:else if !known}
  <PageLoading />
{:else}
  <Group title="Everywhere" more="Out of every crate, and never picked by Radio, Autoplay or a shuffle of the library or the favourites. It still plays when you choose it.">
    <Item name="Excluded everywhere" value={excluded.n === null ? null : count(excluded.n)} onclick={excluded.n ? () => openThing('taste', EVERYWHERE) : null} />
  </Group>
  {#if leaving.length}
    <Group title="Excluded from crates">
      {#each leaving as c (c.id)}
        <Item name={c.name} value={count(c.left_out ?? 0)} onclick={() => openThing('taste', CRATE + c.id)} />
      {/each}
    </Group>
  {/if}
{/if}
