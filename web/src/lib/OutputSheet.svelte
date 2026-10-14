<script lang="ts">
  // Where music plays (decisions 0019, 0022), from the player's output button: this browser, or the server's player.
  // Choosing the other one moves the music there (queue and position) and stops the side that played.
  import { live } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { canPlayHere } from './localPlayer.svelte';
  import { isLocal, toLocal, toRemote } from './output.svelte';
  import { shownCover } from './remote.svelte';

  let { onclose }: { onclose: () => void } = $props();

  // The server's player as the page last saw it ("Finamp on Lenovo Tab"); a plain name when nothing plays there.
  const remoteName = $derived(live.view?.now_playing?.player ?? 'The server’s player');
  const src = $derived(shownCover());

  async function choose(local: boolean) {
    onclose();
    if (local === isLocal()) return;
    if (local) await toLocal();
    else await toRemote();
  }
</script>

<MenuSheet label="Play on" title="Play on" images={src ? [src] : []} chips={[isLocal() ? 'This device' : remoteName]} {onclose}>
  <div role="menu">
    {#if canPlayHere}
      <MenuEntry label="This device" checked={isLocal()} onclick={() => choose(true)} />
    {/if}
    <MenuEntry label={remoteName} checked={!isLocal()} onclick={() => choose(false)} />
  </div>
</MenuSheet>
