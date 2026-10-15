<script lang="ts">
  // Where music plays (decisions 0019, 0022, 0031): this browser (or the app it runs in), one of Cratarr's own players
  // (the Android app elsewhere: Play On), or the media server's player. Choosing another moves the music there (queue
  // and position) and stops the side that played.
  import { live } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { canPlayHere } from './localPlayer.svelte';
  import { moveTo } from './output.svelte';
  import { choosable, drivenName } from './playOn.svelte';
  import { prefs } from './prefs.svelte';
  import { shownCover } from './remote.svelte';
  import { toast } from './toast.svelte';

  let { onclose }: { onclose: () => void } = $props();

  // The server's player as the page last saw it ("Finamp on Lenovo Tab"); a plain name when nothing plays there.
  const remoteName = $derived(live.view?.now_playing?.player ?? 'The server’s player');
  const src = $derived(shownCover());
  const here = $derived(prefs.output === 'local' && canPlayHere);
  const shown = $derived(here ? 'This device' : (drivenName() ?? remoteName));
  // The chosen player stays listed while it is away (offline), so what is chosen shows.
  const players = $derived.by(() => {
    const list = choosable().map((p) => p.name);
    const d = drivenName();
    return d && !list.includes(d) ? [...list, d] : list;
  });

  async function choose(output: typeof prefs.output) {
    onclose();
    try {
      await moveTo(output);
    } catch (e) {
      toast.fail(e, "The music didn't move");
    }
  }
</script>

<MenuSheet label="Play on" title="Play on" images={src ? [src] : []} chips={[shown]} {onclose}>
  <div role="menu">
    {#if canPlayHere}
      <MenuEntry label="This device" checked={here} onclick={() => choose('local')} />
    {/if}
    {#each players as name (name)}
      <MenuEntry label={name} checked={drivenName() === name} onclick={() => choose(`player:${name}`)} />
    {/each}
    <MenuEntry label={remoteName} checked={!here && drivenName() === null} onclick={() => choose('remote')} />
  </div>
</MenuSheet>
