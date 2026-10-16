<script lang="ts">
  // The radio (decision 0036, the player's radio button): its modes as toggle cards, the one on lit. Picking a mode turns
  // the radio on in it; picking the lit one turns the radio off. The player that plays fills the radio's lane.
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { local, RADIO_MODES, setRadio, type RadioMode } from './localPlayer.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const NAMES: Record<RadioMode, string> = {
    similar: 'Similar',
    continuous: 'Continuous',
    album_mix: 'Album mix',
    era: 'Era',
    artist: 'Artist',
    reshuffle: 'Reshuffle',
    random: 'Random',
  };

  function pick(mode: RadioMode) {
    if (local.radio?.mode === mode) setRadio(false);
    else setRadio(true, mode);
    onclose();
  }
</script>

<MenuSheet label="Radio" icon="radio" title="Radio" chips={local.radio ? [NAMES[local.radio.mode]] : []} {onclose}>
  <div role="menu">
    {#each RADIO_MODES as mode (mode)}
      <MenuEntry label={NAMES[mode]} checked={local.radio?.mode === mode} onclick={() => pick(mode)} />
    {/each}
  </div>
</MenuSheet>
