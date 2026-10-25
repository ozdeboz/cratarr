<script lang="ts">
  // Radio and Autoplay (the owner, 2026-10-25: a part of its own under Personal): their defaults, not the playing
  // queue's (that is the More music sheet's, lib/MoreMusicSheet). Auto arm: whether each new queue starts with Autoplay
  // on; how Autoplay and Radio start when switched on from off (they "kick in when autoplay is initialized after first
  // being off"); what makes music alike and leaving favourites out, for both; then what they choose from, apart from
  // what the pages show: each of Cratarr's own libraries, and Bandcamp (Discover's suggestions that play there, heard
  // as previews, decision 0071). Kept for the account (lib/mySettings; the server reads them for every player).
  import { library, readLibrary } from '../albums.svelte';
  import type { AutoplayMode, RadioBy } from '../localPlayer.svelte';
  import { RADIO_BYS, RADIO_MAX } from '../localPlayer.svelte';
  import { AUTOPLAY_NAMES, RADIO_NAMES } from '../MoreMusicSheet.svelte';
  import { defaults, libraryOn, loadMine, mine, setDefaults, setMoreFrom, setSimilarBy, similar, SIGNALS, turnLibrary } from '../mySettings.svelte';
  import { openThing } from '../panel.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import OrderList from './OrderList.svelte';
  import { OWN } from '../sources.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  void loadMine();
  // Similar by on its own page (`thing`: `similar`), as Settings' lists are (lib/settings/OrderList): the ones on in
  // their order, the first counting most; all of them while none is chosen.
  const SIMILAR = 'similar';
  nameThing(() => (thing === SIMILAR ? 'Similar by' : null));
  const equal = $derived(similar.by.length === 0);
  const similarOn = $derived(equal ? SIGNALS.map((s) => s.id) : similar.by);
  const similarLine = $derived(equal ? 'All, equally' : similar.by.map((id) => SIGNALS.find((s) => s.id === id)?.label).join(', '));
  if (!library.view) readLibrary(false).catch(() => {});

  let choosing = $state<'mode' | 'by' | 'count' | null>(null);
  const MODES = (Object.entries(AUTOPLAY_NAMES) as [AutoplayMode, string][]).map(([value, label]) => ({ value, label }));
  const BYS = RADIO_BYS.map((by) => ({ value: by, label: RADIO_NAMES[by] }));
  const COUNTS = Array.from({ length: RADIO_MAX }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }));

  const libraries = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);
</script>

{#if thing === SIMILAR}
  <Group more={equal ? 'All count the same.' : 'The first counts most, the next half as much, and so on.'}>
    <OrderList items={SIGNALS} on={similarOn} label="Similar by" onchange={(on) => on.length && setSimilarBy(on)} />
  </Group>
  {#if !equal}<Group><Item name="All the same" kind="add" onclick={() => setSimilarBy([])} /></Group>{/if}
{:else}
<Group title="Autoplay" more="What plays when the queue ends. Auto arm: a new queue turns Autoplay on when it was off; off, a new queue leaves it as it was. Turned on, it starts as set here.">
  <Item name="Auto arm" toggle={{ checked: defaults.autoplay.arm, onchange: (on) => setDefaults({ autoplay: { arm: on } }) }} />
  <Item name="Plays" value={AUTOPLAY_NAMES[defaults.autoplay.mode]} onclick={() => (choosing = 'mode')} />
  <Item name="Queue albums" toggle={{ checked: defaults.autoplay.albums, onchange: (on) => setDefaults({ autoplay: { albums: on } }) }} />
</Group>
<Group title="Radio" more="Tracks after each of your tracks. Turned on, it starts as set here.">
  <Item name="Like" value={RADIO_NAMES[defaults.radio.by]} onclick={() => (choosing = 'by')} />
  <Item name="After each track" value={String(defaults.radio.count)} onclick={() => (choosing = 'count')} />
</Group>
<Group title="Both" more="What makes music alike for Radio and Autoplay.">
  <Item name="Similar by" value={similarLine} onclick={() => openThing('radio', SIMILAR)} />
  <Item name="Ignore favourites" toggle={{ checked: defaults.skipFavourites, onchange: (on) => setDefaults({ skipFavourites: on }) }} />
</Group>
<Group title="From" more="Where Radio and Autoplay find their music, whatever the pages show. Bandcamp: Discover's suggestions that play there, heard as previews; never counted as plays.">
  {#each libraries as l (l.id)}
    <Item name={l.name} toggle={{ checked: libraryOn(l.id), onchange: (on) => void turnLibrary(l.id, on, libraries.map((x) => x.id)) }} />
  {/each}
  <Item name="Bandcamp" toggle={{ checked: mine.moreFrom.bandcamp, onchange: (on) => void setMoreFrom({ bandcamp: on }) }} />
</Group>
{/if}
{#if choosing === 'mode'}
  <ChoiceSheet title="Autoplay plays" options={MODES} value={defaults.autoplay.mode} onchange={(v: AutoplayMode) => setDefaults({ autoplay: { mode: v } })} onclose={() => (choosing = null)} />
{:else if choosing === 'by'}
  <ChoiceSheet title="Radio like" options={BYS} value={defaults.radio.by} onchange={(v: RadioBy) => setDefaults({ radio: { by: v } })} onclose={() => (choosing = null)} />
{:else if choosing === 'count'}
  <ChoiceSheet title="After each track" options={COUNTS} value={String(defaults.radio.count)} onchange={(v: string) => setDefaults({ radio: { count: Number(v) } })} onclose={() => (choosing = null)} />
{/if}
