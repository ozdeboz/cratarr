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
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import SettingRow from '../SettingRow.svelte';
  import OrderList from './OrderList.svelte';
  import { OWN } from '../sources.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  void loadMine();
  // Similar by in a sheet, as Radio's Like (the owner, 2026-10-25), its list as Settings' lists are
  // (lib/settings/OrderList): the ones on in their order, the first counting most; all of them while none is chosen.
  const equal = $derived(similar.by.length === 0);
  const similarOn = $derived(equal ? SIGNALS.map((s) => s.id) : similar.by);
  const similarLine = $derived(equal ? 'All, equally' : similar.by.map((id) => SIGNALS.find((s) => s.id === id)?.label).join(', '));
  if (!library.view) readLibrary(false).catch(() => {});

  let choosing = $state<'mode' | 'by' | 'count' | 'similar' | null>(null);
  const MODES = (Object.entries(AUTOPLAY_NAMES) as [AutoplayMode, string][]).map(([value, label]) => ({ value, label }));
  const BYS = RADIO_BYS.map((by) => ({ value: by, label: RADIO_NAMES[by] }));
  // How many after each track: a slider in a sheet (the owner, 2026-10-25), as the More music sheet's; the number
  // follows the finger and is kept when it lifts.
  let dragging = $state<number | null>(null);

  const libraries = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);
</script>

<Group title="Autoplay" more="What plays when the queue ends. Auto arm: a new queue turns Autoplay on when it was off; off, a new queue leaves it as it was. Turned on, it starts as set here.">
  <Item name="Auto arm" toggle={{ checked: defaults.autoplay.arm, onchange: (on) => setDefaults({ autoplay: { arm: on } }) }} />
  <Item name="Plays" value={AUTOPLAY_NAMES[defaults.autoplay.mode]} onclick={() => (choosing = 'mode')} />
  <Item name="Queue albums" toggle={{ checked: defaults.autoplay.albums, onchange: (on) => setDefaults({ autoplay: { albums: on } }) }} />
</Group>
<Group title="Radio" more="Tracks after each of your tracks. Turned on, it starts as set here.">
  <Item name="Like" value={RADIO_NAMES[defaults.radio.by]} onclick={() => (choosing = 'by')} />
  <Item name="After each track" value={String(defaults.radio.count)} onclick={() => (choosing = 'count')} />
</Group>
<Group title="Shared" more="What makes music alike for Radio and Autoplay.">
  <Item name="Similar by" value={similarLine} onclick={() => (choosing = 'similar')} />
  <Item name="Ignore favourites" toggle={{ checked: defaults.skipFavourites, onchange: (on) => setDefaults({ skipFavourites: on }) }} />
</Group>
<Group title="From" more="Where Radio and Autoplay find their music, whatever the pages show. Bandcamp: Discover's suggestions that play there, heard as previews; never counted as plays.">
  {#each libraries as l (l.id)}
    <Item name={l.name} toggle={{ checked: libraryOn(l.id), onchange: (on) => void turnLibrary(l.id, on, libraries.map((x) => x.id)) }} />
  {/each}
  <Item name="Bandcamp" toggle={{ checked: mine.moreFrom.bandcamp, onchange: (on) => void setMoreFrom({ bandcamp: on }) }} />
</Group>
{#if choosing === 'similar'}
  <MenuSheet label="Similar by" title="Similar by" chips={[similarLine]} onclose={() => (choosing = null)}>
    <div class="similar">
      <Group more={equal ? 'All count the same.' : 'The first counts most, the next half as much, and so on.'}>
        <OrderList items={SIGNALS} on={similarOn} label="Similar by" onchange={(on) => on.length && setSimilarBy(on)} />
      </Group>
      {#if !equal}<Group><Item name="All the same" kind="add" onclick={() => setSimilarBy([])} /></Group>{/if}
    </div>
  </MenuSheet>
{:else if choosing === 'mode'}
  <ChoiceSheet title="Autoplay plays" options={MODES} value={defaults.autoplay.mode} onchange={(v: AutoplayMode) => setDefaults({ autoplay: { mode: v } })} onclose={() => (choosing = null)} />
{:else if choosing === 'by'}
  <ChoiceSheet title="Radio like" options={BYS} value={defaults.radio.by} onchange={(v: RadioBy) => setDefaults({ radio: { by: v } })} onclose={() => (choosing = null)} />
{:else if choosing === 'count'}
  <MenuSheet label="After each track" title="After each track" onclose={() => (choosing = null)}>
    <div class="count">
      <SettingRow label="After each track">
        <input
          type="range"
          use:panSafe
          min="1"
          max={RADIO_MAX}
          step="1"
          value={dragging ?? defaults.radio.count}
          aria-label="Tracks after each track"
          oninput={(e) => (dragging = Number(e.currentTarget.value))}
          onchange={(e) => {
            setDefaults({ radio: { count: Number(e.currentTarget.value) } });
            dragging = null;
          }}
        />
        <output>{dragging ?? defaults.radio.count}</output>
      </SettingRow>
    </div>
  </MenuSheet>
{/if}

<style>
  /* The sheet's list, as a part's groups. */
  .similar {
    padding: 0 16px 16px;
  }

  /* The slider, its number after it, as the More music sheet's. */
  .count {
    padding: 0 16px 16px;
  }

  /* The sheet's one slider, as wide as the sheet (the owner's iPhone, 2026-10-27: a short one under its name). */
  .count :global(.control) {
    flex: 1 1 14em;
  }

  .count input {
    flex: 1;
    width: auto;
    min-width: 0;
    accent-color: var(--accent);
  }

  .count output {
    width: 2ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
