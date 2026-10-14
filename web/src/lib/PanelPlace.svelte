<script lang="ts">
  // Back beside the page (decision 0018): the full-screen player's button in its top right corner, in every panel's top
  // right corner too, so whatever the panel shows (a playlist, a crate, the builder) it can go back beside the page
  // without the player. Only while the panel is over the page on a window with room for it beside the page.
  import Icon from './Icon.svelte';
  import { player } from './live.svelte';
  import { prefs } from './prefs.svelte';

  function besideThePage() {
    prefs.sidebar = true;
    player.full = false;
  }
</script>

{#if player.roomy && !prefs.sidebar}
  <button class="place" aria-label="Beside the page" title="Beside the page" onclick={besideThePage}><Icon name="sidebar" size={22} /></button>
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
