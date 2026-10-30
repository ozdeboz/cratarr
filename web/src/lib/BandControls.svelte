<script lang="ts">
  // The bands' settings (the owner, 2026-10-24), the same in the player's sheet (lib/BandSheet) and in Settings ›
  // Display › Player visualisation: the look, its reset, and its settings, each its name above it and as wide as where
  // it is. Each look has its own (lib/bandTune). Switching to a look whose settings differ from the one left asks once
  // which to go on with (a takeover, the choice sheet as everywhere: lib/MenuSheet): its own, shown at once, or a copy of these,
  // carried over. Its reset asks first; a long
  // press resets every look to its own defaults (asking too).
  import BandSlider, { bandValueText } from './BandSlider.svelte';
  import Item from './settings/Item.svelte';
  import { BAND_DIRECTIONS, BAND_ENGINES, BAND_LOOKS, BANDS_MORE, COUNT_MORE, ENGINE_MORE, WINDOW_MORE } from './bandWaves.svelte';
  import { resetTune, setTune, tuneIsDefault, tuneOf } from './bandTune.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import ChoiceSheet from './settings/ChoiceSheet.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MoreInfo from './MoreInfo.svelte';
  import { type BandLookName, type BandTune, TUNE_RANGES, prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';

  const look = $derived(prefs.playerBands === 'off' ? null : prefs.playerBands);
  const tune = $derived(look ? tuneOf(look) : null);
  const named = (l: BandLookName) => BAND_LOOKS.find((o) => o.value === l)?.label ?? l;

  /** A switch from a look whose settings differ: the ones left, to carry over if asked. */
  let takeover = $state<{ to: BandLookName; from: BandLookName; tune: BandTune } | null>(null);
  function choose(to: Prefs['playerBands']) {
    const from = look;
    prefs.playerBands = to;
    takeover = from && to !== 'off' && to !== from && JSON.stringify(tuneOf(from)) !== JSON.stringify(tuneOf(to)) ? { to, from, tune: tuneOf(from) } : null;
  }
  function keep() {
    if (!takeover) return;
    for (const k of Object.keys(takeover.tune) as (keyof BandTune)[]) setTune(takeover.to, k, takeover.tune[k]);
    takeover = null;
  }

  /** One of the look's settings, bound: read from its tune, written as its override. */
  const bind = <K extends keyof BandTune>(key: K) => ({
    get: () => (tune as BandTune)[key],
    set: (v: BandTune[K]) => look && setTune(look, key, v),
  });
  const direction = bind('direction');
  /** The sliders, in their order (Direction's choice goes before Taper): the player's sheet shows each inline, Settings
   *  a row with its value opening its sheet (the owner, 2026-10-25: as Settings' other sliders). */
  type Slider = { key: 'count' | 'speed' | 'window' | 'cycles' | 'height' | 'gap' | 'taper' | 'fade'; label: string; ends: [string, string]; unit?: string; linear?: boolean; whole?: boolean; more?: string; not?: BandLookName[] };
  const SLIDERS: Slider[] = [
    { key: 'count', label: 'Bands', ends: ['3', '32'], whole: true, more: COUNT_MORE },
    { key: 'speed', label: 'Speed', ends: ['Ultra slow', 'Radical'] },
    { key: 'window', label: 'Window', ends: ['Beat', 'Phrase'], unit: 's', more: WINDOW_MORE },
    { key: 'cycles', label: 'Cycles', ends: ['Few', 'Many'], not: ['meter'] },
    { key: 'height', label: 'Height', ends: ['Low', 'Tall'] },
    { key: 'gap', label: 'Gap', ends: ['None', 'Wide'], linear: true },
    { key: 'taper', label: 'Taper', ends: ['None', 'Full'], linear: true },
    { key: 'fade', label: 'Fade', ends: ['None', 'Wide'], linear: true },
  ];
  const valueOf = (x: Slider) => (tune ? `${bandValueText(tune[x.key] as number, x.whole)}${x.unit ? ` ${x.unit}` : ''}` : '');
  /** `rows`: as Settings' rows (lib/settings/Item), each slider in its own sheet; else inline (the player's sheet). */
  let { rows = false }: { rows?: boolean } = $props();
  /** The slider whose sheet is open (rows). */
  let sliding = $state<Slider | null>(null);

  type Prefs = typeof prefs;

  /** A reset asked about: this look's, or every look's. */
  let asking = $state<'look' | 'all' | null>(null);
  /** Five looks: the row says the one, the choice sheet picks it (the owner, 2026-10-25). */
  let picking = $state(false);
  const anyChanged = $derived(Object.values(prefs.playerBandsTunes).some((t) => t && Object.keys(t).length));
</script>

{#snippet slider(x: Slider)}
  {@const b = bind(x.key)}
  <BandSlider label={x.label} range={TUNE_RANGES[x.key]} bind:value={b.get, b.set} ends={x.ends} unit={x.unit ?? null} linear={x.linear} whole={x.whole} wide />
{/snippet}
{#snippet directionRow()}
  <div class="row beside">
    {@render name('Direction')}
    <Seg label="Direction" options={BAND_DIRECTIONS} bind:value={direction.get, direction.set} />
  </div>
{/snippet}

{#snippet name(label: string, more: string | null = null)}
  <span class="name">{label}{#if more}<MoreInfo {label} text={more} />{/if}</span>
{/snippet}

{#if rows}
  <!-- Settings' rows (the owner, 2026-10-25): the look a choice, each slider its value opening its sheet, Direction and
       the engine short choices beside their names, the resets last. -->
  <Item name="Look" more={BANDS_MORE} value={BAND_LOOKS.find((o) => o.value === prefs.playerBands)?.label ?? null} onclick={() => (picking = true)} />
  {#if look && tune}
    {#each SLIDERS.filter((x) => !x.not?.includes(look)) as x (x.key)}
      {#if x.key === 'taper'}<Item name="Direction"><Seg label="Direction" options={BAND_DIRECTIONS} bind:value={direction.get, direction.set} /></Item>{/if}
      <Item name={x.label} more={x.more ?? null} value={valueOf(x)} onclick={() => (sliding = x)} />
    {/each}
    <Item name="Engine" more={ENGINE_MORE}><Seg label="Engine" options={BAND_ENGINES} bind:value={prefs.playerBandsEngine} /></Item>
    <Item name="Reset {named(look)}" disabled={tuneIsDefault(look)} onclick={() => (asking = 'look')} />
    {#if anyChanged}<Item name="Reset every look" onclick={() => (asking = 'all')} />{/if}
  {/if}
  {#if sliding && look && tune}
    {@const x = sliding}
    <MenuSheet label={x.label} title={x.label} chips={[named(look)]} onclose={() => (sliding = null)}>
      <div class="sheet-slider">{@render slider(x)}</div>
    </MenuSheet>
  {/if}
{:else}
<div class="rows">
  <div class="row">
    <span class="head">
      {@render name('Look', BANDS_MORE)}
      <span class="end">
      {#if look}<button
          class="reset"
          disabled={!anyChanged}
          aria-label="Reset {named(look)} (hold: every look)"
          title="Reset (hold: every look)"
          onclick={() => !tuneIsDefault(look) && (asking = 'look')}
          use:longpress={() => (asking = 'all')}><Icon name="reset" size={18} /></button
        >{/if}
      <button class="pick" aria-haspopup="dialog" aria-label="Look: {BAND_LOOKS.find((o) => o.value === prefs.playerBands)?.label}" onclick={() => (picking = true)}
        >{BAND_LOOKS.find((o) => o.value === prefs.playerBands)?.label}<Icon name="chevronRight" size={18} /></button
      >
      </span>
    </span>
  </div>
  {#if look && tune}
    {#each SLIDERS.filter((x) => !x.not?.includes(look)) as x (x.key)}
      {#if x.key === 'taper'}{@render directionRow()}{/if}
      <div class="row">
        {@render name(x.label, x.more ?? null)}
        {@render slider(x)}
      </div>
    {/each}
    <!-- This device's, for every look (not a look's setting: its reset leaves it). -->
    <div class="row beside">
      {@render name('Engine', ENGINE_MORE)}
      <Seg label="Engine" options={BAND_ENGINES} bind:value={prefs.playerBandsEngine} />
    </div>
  {/if}
</div>
{/if}

{#if picking}
  <ChoiceSheet title="Look" options={BAND_LOOKS} value={prefs.playerBands} onchange={choose} onclose={() => (picking = false)} />
{/if}
{#if takeover && takeover.to === look}
  <MenuSheet label="Settings for {named(takeover.to)}" icon="sliders" title="Settings for {named(takeover.to)}" onclose={() => (takeover = null)}>
    <div role="menu">
      <MenuEntry label="{named(takeover.to)}'s own" onclick={() => (takeover = null)} />
      <MenuEntry label="Copy {named(takeover.from)}'s" onclick={keep} />
    </div>
  </MenuSheet>
{:else if asking === 'look' && look}
  <ConfirmSheet title="Reset {named(look)}?" label="Reset {named(look)}" icon="reset" onconfirm={() => resetTune(look)} onclose={() => (asking = null)} />
{:else if asking === 'all'}
  <ConfirmSheet
    title="Reset every look?"
    text="Aurora, Sirius, Hairlines and Meter each back to its own defaults."
    label="Reset every look"
    icon="reset"
    onconfirm={() => (prefs.playerBandsTunes = {})}
    onclose={() => (asking = null)}
  />
{/if}

<style>
  .reset {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
    touch-action: none;
  }

  .reset:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .rows {
    display: grid;
    gap: 14px;
  }

  /* A slider's sheet: the slider as wide as the sheet. */
  .sheet-slider {
    padding: 8px 24px 20px;
  }

  .row {
    display: grid;
    gap: 6px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 32px;
  }

  .name {
    display: inline-flex;
    align-items: center;
    color: var(--muted);
    font-size: 0.9em;
  }

  /* A short choice beside its name (lib/Seg), not a line of its own (the owner, 2026-10-25). */
  .row.beside {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
  }

  /* The look: its name, at the end the reset and the one chosen, which opens the choices. */
  .head .end {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .pick {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 36px;
    padding: 0 2px 0 10px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

</style>
