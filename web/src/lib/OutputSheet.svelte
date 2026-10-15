<script lang="ts">
  // Where music plays (decisions 0019, 0022, 0031): this browser (or the app it runs in), one of Cratarr's own players
  // (the Android app elsewhere: Play On), or the media server's player. Choosing another moves the music there (queue
  // and position) and stops the side that played; one of Cratarr's players that plays already is asked about first:
  // take it over (the music moves there), or not (it plays on, and what played here pauses).
  import { coverUrl, type PlayOnPlayer } from './api';
  import { live } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { canPlayHere } from './localPlayer.svelte';
  import { hasMusic, joinTo, moveTo } from './output.svelte';
  import { choosable, drivenName, playOn } from './playOn.svelte';
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

  // A player that plays already, while music plays here too: whether to take it over.
  let asking = $state<PlayOnPlayer | null>(null);

  function pick(output: typeof prefs.output) {
    const name = output.startsWith('player:') ? output.slice('player:'.length) : null;
    const there = name === null ? undefined : playOn.players.find((p) => p.name === name);
    if (there?.online && there.playing && output !== prefs.output && hasMusic()) {
      asking = there;
      return;
    }
    choose(output, moveTo);
  }

  async function choose(output: typeof prefs.output, how: (output: typeof prefs.output) => Promise<void>) {
    onclose();
    try {
      await how(output);
    } catch (e) {
      toast.fail(e, "The music didn't move");
    }
  }
</script>

<MenuSheet
  label="Play on"
  title={asking ? asking.name : 'Play on'}
  images={asking ? (asking.track ? [coverUrl(asking.track)] : []) : src ? [src] : []}
  chips={asking ? (asking.track ? [asking.track.name] : []) : [shown]}
  step={asking ? 'ask' : 'menu'}
  onback={asking ? () => (asking = null) : null}
  {onclose}
>
  <div role="menu">
    {#if asking}
      {@const name = asking.name}
      <MenuEntry label="Take over" onclick={() => choose(`player:${name}`, moveTo)} />
      <MenuEntry label="Don't take over" onclick={() => choose(`player:${name}`, joinTo)} />
    {:else}
      {#if canPlayHere}
        <MenuEntry label="This device" checked={here} onclick={() => pick('local')} />
      {/if}
      {#each players as name (name)}
        <MenuEntry label={name} checked={drivenName() === name} onclick={() => pick(`player:${name}`)} />
      {/each}
      <MenuEntry label={remoteName} checked={!here && drivenName() === null} onclick={() => pick('remote')} />
    {/if}
  </div>
</MenuSheet>
