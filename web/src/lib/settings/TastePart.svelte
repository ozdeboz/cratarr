<script lang="ts">
  // Taste (the owner, 2026-10-23: "all the exclusions can be managed" here, and later the weights once AudioMuse
  // has heard everything): what one has said one does not want, in one place. Excluded everywhere (decision 0024,
  // amended: out of every crate and every automatic pick), then each crate that leaves tracks out by hand; each opens
  // its own page here (`thing`: `everywhere`, or `crate:<id>`) with the list to put back (lib/ExcludedList). Then each
  // ranked crate's Not for (a5's fit models, GET /api/crates/fits): what it avoids and its rules, in its sheet
  // (lib/NotForSheet). First (the owner, 2026-10-24): this device's own More music (radio and Autoplay), in its sheet.
  import { getCrateFits, traitLabel, type FitSummary } from '../api';
  import MoreMusicSheet, { moreMusicParts } from '../MoreMusicSheet.svelte';
  import { crateName, crates, loadCrates } from '../crates.svelte';
  import ExcludedList from '../ExcludedList.svelte';
  import { excluded, loadExcluded } from '../exclusions.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import NotForSheet from '../NotForSheet.svelte';
  import PageLoading from '../PageLoading.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  $effect(() => {
    loadExcluded();
    void loadCrates();
  });

  /** Each ranked crate's fit with something in it; null until read (a server without it: none). */
  let fits = $state<FitSummary[] | null>(null);
  const loadFits = () =>
    getCrateFits().then(
      (f) => (fits = f),
      () => (fits = []),
    );
  loadFits();
  let moreMusic = $state(false);
  /** The crate whose Not for sheet is open. */
  let notFor = $state<FitSummary | null>(null);
  /** A fit's line: what it avoids, and its own rules. */
  const fitLine = (f: FitSummary) =>
    [f.avoiding.length ? `Avoiding ${f.avoiding.map(traitLabel).join(' · ')}` : null, f.rules.length ? `${f.rules.length} ${f.rules.length === 1 ? 'rule' : 'rules'}` : null]
      .filter(Boolean)
      .join(' · ') || null;

  const EVERYWHERE = 'everywhere';
  const CRATE = 'crate:';
  /** The crates that leave tracks out by hand, the most first. */
  const leaving = $derived((crates.list ?? []).filter((c) => (c.left_out ?? 0) > 0).sort((a, b) => (b.left_out ?? 0) - (a.left_out ?? 0)));
  const crateAt = $derived(thing?.startsWith(CRATE) ? thing.slice(CRATE.length) : null);
  const known = $derived((crates.list !== null || crates.error !== null) && fits !== null);
  // Titled as what the list is (the owner, 2026-10-23): Taste › Exclusions: Unlisted.
  nameThing(() => (thing === EVERYWHERE ? 'Exclusions: Everywhere' : crateAt ? `Exclusions: ${crateName(crateAt)}` : null));
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
  <!-- Headed as what they are (the owner, 2026-10-24): Radio and Autoplay; Exclusions, everywhere and each crate's. -->
  <Group title="Radio and Autoplay" more="What plays besides what you chose, on this device's own player: Radio's tracks after each track, and Autoplay when the queue ends.">
    <Item name="More music" value={moreMusicParts().map((p) => p.text).join(' · ') || 'Off'} onclick={() => (moreMusic = true)} />
  </Group>
  <Group title="Exclusions" more="Everywhere: out of every crate, and never picked by Radio, Autoplay or a shuffle of the library or the favourites; it still plays when you choose it. A crate's: left out of that crate only.">
    <Item name="Everywhere" value={excluded.n === null ? null : count(excluded.n)} onclick={excluded.n ? () => openThing('taste', EVERYWHERE) : null} />
    {#each leaving as c (c.id)}
      <Item name={c.name} value={count(c.left_out ?? 0)} onclick={() => openThing('taste', CRATE + c.id)} />
    {/each}
  </Group>
  {#if fits?.length}
    <Group title="Not for" more="What each ranked crate has learned not to suggest, from what was turned down for it, and the rules you wrote.">
      {#each fits as f (f.crate)}
        <Item name={f.of} line={fitLine(f)} onclick={() => (notFor = f)} />
      {/each}
    </Group>
  {/if}
{/if}
{#if moreMusic}
  <MoreMusicSheet onclose={() => (moreMusic = false)} />
{/if}
{#if notFor}
  <NotForSheet crate={notFor.crate} name={notFor.of} onchange={loadFits} onclose={() => (notFor = null)} />
{/if}
