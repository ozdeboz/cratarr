<script lang="ts">
  // The bands' settings from the player itself (the owner, 2026-10-24: a tap on them opens a sheet with their
  // settings): the look, Speed and Window, the same as Settings › Display › Player visualisation, acting at once.
  import BandSlider from './BandSlider.svelte';
  import { BAND_DIRECTIONS, BAND_ENDS, BAND_LOOKS, BANDS_MORE, COUNT_MORE, WINDOW_MORE } from './bandWaves.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { BAND_COUNT, BAND_CYCLES, BAND_GAP, BAND_HEIGHT, BAND_PACE, BAND_WINDOW, prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SettingRow from './SettingRow.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Player visualisation" icon="sliders" title="Player visualisation" {onclose}>
  <div class="rows">
    <SettingRow label="Bands" more={BANDS_MORE}><Seg label="Bands" options={BAND_LOOKS} bind:value={prefs.playerBands} /></SettingRow>
    {#if prefs.playerBands !== 'off'}
      <SettingRow label="Bands" more={COUNT_MORE}><BandSlider label="Bands" range={BAND_COUNT} bind:value={prefs.playerBandsCount} ends={['3', '32']} whole /></SettingRow>
      <SettingRow label="Speed"><BandSlider label="Speed" range={BAND_PACE} bind:value={prefs.playerBandsSpeed} ends={['Calm', 'Radical']} /></SettingRow>
      <SettingRow label="Window" more={WINDOW_MORE}><BandSlider label="Window" range={BAND_WINDOW} bind:value={prefs.playerBandsWindow} ends={['Beat', 'Phrase']} unit="s" /></SettingRow>
      <SettingRow label="Cycles"><BandSlider label="Cycles" range={BAND_CYCLES} bind:value={prefs.playerBandsCycles} ends={['Few', 'Many']} /></SettingRow>
      <SettingRow label="Height"><BandSlider label="Height" range={BAND_HEIGHT} bind:value={prefs.playerBandsHeight} ends={['Low', 'Tall']} /></SettingRow>
      <SettingRow label="Gap"><BandSlider label="Gap" range={BAND_GAP} bind:value={prefs.playerBandsGap} ends={['None', 'Wide']} linear /></SettingRow>
      <SettingRow label="Direction"><Seg label="Direction" options={BAND_DIRECTIONS} bind:value={prefs.playerBandsDirection} /></SettingRow>
      <SettingRow label="Ends"><Seg label="Ends" options={BAND_ENDS} bind:value={prefs.playerBandsEnds} /></SettingRow>
    {/if}
  </div>
</MenuSheet>

<style>
  .rows {
    display: grid;
    gap: 4px;
    padding: 4px 16px 16px;
  }
</style>
