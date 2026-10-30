<script lang="ts">
  // Where the panel shows (decision 0018): beside the page or over it. The player's top-right button, in every panel's
  // top-right corner too (a playlist, a crate, the builder), so the place can change whatever the panel shows: beside
  // the page "Over the page", over it "Beside the page". Only on a window with room for the panel beside the page, and
  // not where the page keeps the panel beside its list (Settings, Health, Insights, Downloads: the owner, 2026-10-24).
  // Only the place changes: the player stays as it is (folded to the mini-player when it was).
  import Icon from './Icon.svelte';
  import { player } from './live.svelte';
  import { panel } from './panel.svelte';
  import { prefs } from './prefs.svelte';

  function overThePage() {
    prefs.sidebar = false;
  }
  function besideThePage() {
    prefs.sidebar = true;
    player.full = false;
  }
</script>

{#if player.roomy && !panel.split}
  {#if prefs.sidebar}
    <button class="place" aria-label="Over the page" title="Over the page" onclick={overThePage}><Icon name="expand" size={20} /></button>
  {:else}
    <button class="place" aria-label="Beside the page" title="Beside the page" onclick={besideThePage}><Icon name="sidebar" size={22} /></button>
  {/if}
{/if}

<style>
  .place {
    flex: none;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }
</style>
