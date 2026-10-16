<script lang="ts">
  // Where music plays (decisions 0019, 0022, 0031): this browser (or the app it runs in), one of Cratarr's own players
  // (the Android app elsewhere: Play On), or the media server's player. Choosing another moves the music there (queue
  // and position) and stops the side that played; one of Cratarr's players that plays already is asked about first:
  // take it over (the music moves there), or not (it plays on, and what played here pauses).
  // Sync play (decision 0037): while one of Cratarr's players is played on, another's + adds it to its group and its
  // tick takes it out (a long press on it too); the group's players show ticked, its leader crowned, and their volumes
  // below. A tap on a name plays on that one player alone: the group ends.
  import { appInfo, inApp } from './app.svelte';
  import { coverUrl, type PlayOnPlayer } from './api';
  import GroupVolume from './GroupVolume.svelte';
  import { hold } from './hold';
  import { live } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { canPlayHere } from './localPlayer.svelte';
  import { hasMusic, joinTo, moveTo } from './output.svelte';
  import { choosable, drivenName, endGroup, groupOf, head, outputName, playOn, toggleGroup } from './playOn.svelte';
  import { prefs } from './prefs.svelte';
  import { shownCover } from './remote.svelte';
  import { toast } from './toast.svelte';

  let { onclose }: { onclose: () => void } = $props();

  /** A long press, as the rest of the page's (a track's menu). */
  const HOLD_MS = 500;

  // The server's player as the page last saw it ("Finamp on Lenovo Tab"); a plain name when nothing plays there.
  const remoteName = $derived(live.view?.now_playing?.player ?? 'The server’s player');
  const src = $derived(shownCover());
  const here = $derived(prefs.output === 'local' && canPlayHere);
  const shown = $derived(here || drivenName() !== null ? outputName() : remoteName);
  const group = $derived(groupOf(head()));
  // Holding groups: offered while one of Cratarr's players is played on (in the app, this device is one).
  // A group's leader is one of the app's players: a Kodi follows, never leads.
  const kindOf = (n: string | null) => playOn.players.find((p) => p.name === n)?.kind ?? 'app';
  // A page in a browser has no sync play: it is never in a group.
  const groupable = (name: string | null) =>
    name !== null && head() !== null && name !== head() && kindOf(group[0] ?? head()) === 'app' && kindOf(name) !== 'browser';
  // The chosen player stays listed while it is away (offline), so what is chosen shows.
  const players = $derived.by(() => {
    const list = choosable().map((p) => p.name);
    const d = drivenName();
    return d && !list.includes(d) ? [...list, d] : list;
  });

  // A player that plays already, while music plays here too: whether to take it over.
  let asking = $state<PlayOnPlayer | null>(null);

  function pick(output: typeof prefs.output) {
    if (group.length > 1) {
      onclose();
      endGroup()
        .then(() => choose(output, moveTo))
        .catch((e) => toast.fail(e, "The group didn't end"));
      return;
    }
    const name = output.startsWith('player:') ? output.slice('player:'.length) : null;
    const there = name === null ? undefined : playOn.players.find((p) => p.name === name);
    if (there?.online && there.playing && output !== prefs.output && hasMusic()) {
      asking = there;
      return;
    }
    choose(output, moveTo);
  }

  function held(name: string) {
    toggleGroup(name).catch((e) => toast.fail(e, group.includes(name) ? "It didn't leave the group" : "It didn't join"));
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
        {@const me = inApp ? appInfo.player : null}
        <div use:hold={{ start: () => me && held(me), end: () => {}, enabled: groupable(me), ms: HOLD_MS }}>
          <MenuEntry
            label="This device"
            checked={here || (me !== null && group.includes(me))}
            badge={me !== null && group.length > 1 && group[0] === me ? 'crown' : null}
            badgeLabel="Leads the group"
            onmark={groupable(me) ? () => me && held(me) : null}
            markLabel={me !== null && group.includes(me) ? 'Take out of the group' : 'Add to the group'}
            onclick={() => pick('local')}
          />
        </div>
      {/if}
      {#each players as name (name)}
        <div use:hold={{ start: () => held(name), end: () => {}, enabled: groupable(name), ms: HOLD_MS }}>
          <MenuEntry
            label={name}
            checked={drivenName() === name || group.includes(name)}
            badge={group.length > 1 && group[0] === name ? 'crown' : null}
            badgeLabel="Leads the group"
            onmark={groupable(name) ? () => held(name) : null}
            markLabel={group.includes(name) ? 'Take out of the group' : 'Add to the group'}
            onclick={() => pick(`player:${name}`)}
          />
        </div>
      {/each}
      <!-- The server's own player; not when what plays there is one of Cratarr's players listed above (the same one twice). -->
      {#if !playOn.players.some((p) => p.name === remoteName)}
        <MenuEntry label={remoteName} checked={!here && drivenName() === null} onclick={() => pick('remote')} />
      {/if}
      {#if group.length > 1}<GroupVolume names={group} />{/if}
    {/if}
  </div>
</MenuSheet>
