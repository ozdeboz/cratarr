<script lang="ts">
  // The bands' settings from the player itself (the owner, 2026-10-24: a tap on them opens a sheet with their
  // settings): the same controls as Settings › Display › Player visualisation (lib/BandControls), acting at once.
  import BandControls from './BandControls.svelte';
  import MenuSheet from './MenuSheet.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Player visualisation" icon="sliders" title="Player visualisation" {onclose}>
  <div class="body"><BandControls /></div>
</MenuSheet>

<style>
  .body {
    padding: 4px 16px 20px;
  }
</style>
