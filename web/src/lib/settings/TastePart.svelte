<script lang="ts">
  // Taste (the owner, 2026-10-23: "all the exclusions can be managed" here, and later the weights once AudioMuse
  // has heard everything): what one has said one does not want, in one place. Excluded everywhere (decision 0024,
  // amended: out of every crate and every automatic pick), then each crate that leaves tracks out by hand; each opens
  // its own page here (`thing`: `everywhere`, or `crate:<id>`) with the list to put back (lib/ExcludedList). Then each
  // ranked crate's Not for (fit models, GET /api/crates/fits): what it avoids and its rules, in its sheet
  // (lib/NotForSheet). Radio and Autoplay are their own part since 0.9.5 (lib/settings/RadioPart).
  // First the profile as it is built (the owner, 2026-10-25: "one place to adjust it"): what it is built from, each
  // switched as Insights' Profile switches them (the same choice), how much of it AudioMuse has heard, and the profile
  // itself. Discover's own exclusions sit with the others (`thing` `discover`: lib/DiscoveryList's).
  import { getCrateFits, getDiscoverExcluded, getTasteProfile, traitLabel, type FitSummary, type TasteProfile } from '../api';
  import DiscoveryList from '../DiscoveryList.svelte';
  import { PROFILE_SOURCES } from '../insightsControls.svelte';
  import { prefs } from '../prefs.svelte';
  import { crateName, crates, loadCrates } from '../crates.svelte';
  import ExcludedList from '../ExcludedList.svelte';
  import ExcludedSheet from '../ExcludedSheet.svelte';
  import { excluded, loadExcluded } from '../exclusions.svelte';
  import { excludedArtists, loadExcludedArtists, setExcludedEverywhere } from '../excludedArtists.svelte';
  import { when } from './when';
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
  /** The exclusions open in a sheet (the owner, 2026-10-24: not another page): everywhere's (`EVERYWHERE`) or a
   *  crate's id. */
  let excludedOf = $state<string | null>(null);
  /** The crate whose Not for sheet is open. */
  let notFor = $state<FitSummary | null>(null);
  /** A fit's line: what it avoids, and its own rules. */
  const fitLine = (f: FitSummary) =>
    [f.avoiding.length ? `Avoiding ${f.avoiding.map(traitLabel).join(' · ')}` : null, f.rules.length ? `${f.rules.length} ${f.rules.length === 1 ? 'rule' : 'rules'}` : null]
      .filter(Boolean)
      .join(' · ') || null;

  // The profile, of everything (all time, every library), from the sources switched on.
  let profile = $state<TasteProfile | null>(null);
  $effect(() => {
    const off = Object.entries(prefs.profileSources).flatMap(([k, on]) => (on ? [] : [k]));
    let gone = false;
    getTasteProfile(undefined, [], 'all', off).then(
      (p) => !gone && (profile = p),
      () => !gone && (profile = null),
    );
    return () => (gone = true);
  });
  const fed = (id: string) => profile?.taste.sources[id] ?? profile?.avoid.sources[id] ?? null;
  const heard = $derived(profile ? profile.taste.heard + profile.avoid.heard : 0);
  const tracksIn = $derived(profile ? profile.taste.tracks + profile.avoid.tracks : 0);
  function openProfile() {
    prefs.insightsTab = 'listening';
    prefs.listeningView = 'profile';
    location.hash = '#/insights';
  }
  // Discover's own: albums and artists it leaves out.
  let discoverOut = $state<number | null>(null);
  const loadDiscoverOut = () =>
    getDiscoverExcluded().then(
      (r) => (discoverOut = r.albums.length + r.artists.length),
      () => (discoverOut = null),
    );
  loadDiscoverOut();

  const EVERYWHERE = 'everywhere';
  const DISCOVER = 'discover';
  // The artists excluded everywhere (0.10, decision 0083): their own page, each to take back.
  const ARTISTS = 'artists';
  loadExcludedArtists(true);
  const CRATE = 'crate:';
  /** The crates that leave tracks out by hand, the most first. */
  const leaving = $derived((crates.list ?? []).filter((c) => (c.left_out ?? 0) > 0).sort((a, b) => (b.left_out ?? 0) - (a.left_out ?? 0)));
  const crateAt = $derived(thing?.startsWith(CRATE) ? thing.slice(CRATE.length) : null);
  const known = $derived((crates.list !== null || crates.error !== null) && fits !== null);
  // Titled as what the list is (the owner, 2026-10-23): Taste › Exclusions: Unlisted.
  nameThing(() => (thing === EVERYWHERE ? 'Exclusions: Everywhere' : thing === DISCOVER ? 'Exclusions: Discover' : thing === ARTISTS ? 'Exclusions: Artists' : crateAt ? `Exclusions: ${crateName(crateAt)}` : null));
  // A page whose list is all put back, or a thing there is no page for: back to the part.
  const done = () => backFromThing('taste');
  $effect(() => {
    if (thing && thing !== EVERYWHERE && thing !== DISCOVER && thing !== ARTISTS && !crateAt) done();
    if (thing === ARTISTS && excludedArtists.everywhere?.length === 0) done();
  });

  const count = (n: number) => n.toLocaleString();
</script>

{#if thing === EVERYWHERE}
  <ExcludedList onchange={loadExcluded} onempty={done} />
{:else if thing === DISCOVER}
  <DiscoveryList kind="excluded" />
{:else if thing === ARTISTS}
  <Group>
    {#each [...(excludedArtists.everywhere ?? [])].sort((a, b) => a.name.localeCompare(b.name)) as a (a.key)}
      <Item name={a.name} line={a.at ? `Since ${when(a.at)}` : null}><button onclick={() => setExcludedEverywhere(a.name, false)}>Take back</button></Item>
    {/each}
  </Group>
{:else if crateAt}
  <ExcludedList crate={crateAt} onchange={() => loadCrates()} onempty={done} />
{:else if !known}
  <PageLoading />
{:else}
  <Group title="Profile" more="Your taste as Cratarr reads it: more of what you favourite, keep in playlists and play, less of what you skip or exclude. Switched off, a source no longer shapes it.">
    {#each PROFILE_SOURCES as s (s.id)}
      {@const n = fed(s.id)}
      <Item
        name={s.label}
        line={n === null ? s.line : `${s.line} · ${count(n)} ${n === 1 ? 'track' : 'tracks'}`}
        toggle={{ checked: prefs.profileSources[s.id], onchange: (on) => (prefs.profileSources[s.id] = on) }}
      />
    {/each}
    {#if profile && tracksIn}<Item name="Analysed" value={`${count(heard)} of ${count(tracksIn)}`} progress={heard / tracksIn} line={`${Math.round((100 * heard) / tracksIn)}%`} />{/if}
    <Item name="Your profile" onclick={openProfile} />
  </Group>
  <!-- Headed as what they are (the owner, 2026-10-24): Exclusions, everywhere and each crate's. -->
  <Group title="Exclusions" more="Everywhere: out of every crate, and never picked by Radio, Autoplay or a shuffle of the library or the favourites; it still plays when you choose it. A crate's: left out of that crate only.">
    <Item name="Everywhere" value={excluded.n === null ? null : count(excluded.n)} onclick={excluded.n ? () => (excludedOf = EVERYWHERE) : null} />
    {#if excludedArtists.everywhere?.length}<Item name="Artists" line="Out of every crate, radio, Autoplay and Discover" value={count(excludedArtists.everywhere.length)} onclick={() => openThing('taste', ARTISTS)} />{/if}
    {#if discoverOut}<Item name="Discover" value={count(discoverOut)} onclick={() => openThing('taste', DISCOVER)} />{/if}
    {#each leaving as c (c.id)}
      <Item name={c.name} value={count(c.left_out ?? 0)} onclick={() => (excludedOf = c.id)} />
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
{#if excludedOf}
  <ExcludedSheet
    crate={excludedOf === EVERYWHERE ? null : excludedOf}
    onchange={() => (excludedOf === EVERYWHERE ? loadExcluded() : loadCrates())}
    onclose={() => (excludedOf = null)}
  />
{/if}
{#if notFor}
  <NotForSheet crate={notFor.crate} name={notFor.of} onchange={loadFits} onclose={() => (notFor = null)} />
{/if}
