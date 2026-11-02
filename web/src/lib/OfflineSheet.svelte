<script lang="ts">
  // Download…'s gear (the owner, 2026-11-02: the controls that concern downloading, from where one downloads): how this
  // device keeps music for offline listening, the same rows as Settings → Sync (lib/settings/SyncPart), over the step.
  import MenuSheet from './MenuSheet.svelte';
  import SyncPart from './settings/SyncPart.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<MenuSheet label="Offline listening" icon="arrowDownToLine" title="Offline listening" {onclose}>
  <div class="rows">
    <SyncPart />
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
