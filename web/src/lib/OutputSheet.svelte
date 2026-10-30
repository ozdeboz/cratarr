<script lang="ts">
  // Where music plays (decisions 0019, 0022, 0031): this browser (or the app it runs in), one of Cratarr's own players
  // (the Android app elsewhere: Play On), or the media server's player. Choosing another moves the music there (queue
  // and position) and stops the side that played; one of Cratarr's players that plays already is asked about first:
  // take it over (the music moves there), or not (it plays on, and what played here pauses).
  // Sync play (decision 0037): while one of Cratarr's players is played on, another's + adds it to its group and its
  // tick takes it out (a long press on it too); the group's players show ticked, its leader crowned, and their volumes
  // below. A tap on a name plays on that one player alone: the group ends. One that cannot join shows no + (the owner,
  // 2026-10-23: a + that played there alone read as "add to the group"), only its tick when chosen.
  // This device's sound (decision 0033, amended 2026-10-19; the owner: "hardware audio device selection in case of local
  // playback, even in a group"): under This device while it plays, alone or in a group, the output it sounds on; a tap
  // lists the app's outputs, Automatic (Android's own choice) first. A choice holds until an output comes or goes.
  // Following a group, the group's player (Snapcast) makes the sound and takes no choice (the owner: "for later, if
  // ever"): the row shows greyed. Automatic can follow the user's order (the owner, 2026-10-19: "Automatic follows it but
  // we need a toggle where we can also turn off the override. When override is turned off the Android order comes back
  // and the drag bars remove"): "My order" on, every output seen is listed in that order with its handle, those not
  // connected greyed (a swipe left forgets one); off, the connected ones only, as Android picks.
  import { appInfo, inApp, setDevice } from './app.svelte';
  import { coverUrl, type AudioOutput, type PlayOnPlayer } from './api';
  import Icon, { type IconName } from './Icon.svelte';
  import { Reorder } from './reorder.svelte';
  import { swipe } from './swipe';
  import SwipeBehind from './SwipeBehind.svelte';
  import GroupVolume from './GroupVolume.svelte';
  import { hold } from './hold';
  import { live } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { canPlayHere } from './localPlayer.svelte';
  import { hasMusic, joinTo, moveTo } from './output.svelte';
  import { choosable, drivenName, endGroup, groupOf, head, outputName, playOn, selfName, toggleGroup } from './playOn.svelte';
  import { prefs } from './prefs.svelte';
  import { shownCover } from './remote.svelte';
  import { toast } from './toast.svelte';
  import { follow, followingNow, followOffered, loadFollow } from './following.svelte';
  import { followOn, stopFollowing } from './api';
  import { mine, setAutoAccept, setFollowable } from './mySettings.svelte';

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
  // A page in a browser follows, as a Kodi does (lib/groupListen).
  const groupable = (name: string | null) => name !== null && head() !== null && name !== head() && kindOf(group[0] ?? head()) === 'app';
  // The chosen player stays listed while it is away (offline), so what is chosen shows.
  const players = $derived.by(() => {
    const list = choosable().map((p) => p.name);
    const d = drivenName();
    return d && !list.includes(d) ? [...list, d] : list;
  });

  // What another player plays (the owner, 2026-10-23: "Each other player's row should show what it's playing. But not
  // the cover"): its track and artist, playing or paused; nothing while it holds none.
  function playing(name: string): { text: string; playing: boolean } | null {
    const p = playOn.players.find((x) => x.name === name);
    if (!p?.online || !p.title) return null;
    return { text: p.artist ? `${p.title} · ${p.artist}` : p.title, playing: p.playing };
  }

  // A player that plays already, while music plays here too: whether to take it over.
  let asking = $state<PlayOnPlayer | null>(null);

  const me = $derived(selfName());
  const sounding = $derived(inApp && appInfo.outputs.length > 0 && (here || (me !== null && group.includes(me))));
  const following = $derived(me !== null && group.length > 1 && group.includes(me) && group[0] !== me);
  /** Where it sounds: the output chosen, else the one Android chose, as the app reports it. */
  const sounds = $derived<AudioOutput | null>(appInfo.outputs.find((o) => o.id === appInfo.output) ?? playOn.players.find((p) => p.name === me)?.output ?? null);
  const OUT_ICON: Record<AudioOutput['type'], IconName> = { bluetooth: 'bluetooth', speaker: 'smartphone', wired: 'headphones', usb: 'usb', hdmi: 'tv', remote: 'cast', other: 'speaker' };
  const OUT_NAME: Record<AudioOutput['type'], string> = { bluetooth: 'Bluetooth', speaker: 'Speaker', wired: 'Headphones', usb: 'USB', hdmi: 'HDMI', remote: 'Cast', other: 'Other' };
  const outName = (o: AudioOutput) => o.name ?? OUT_NAME[o.type];
  let choosingOutput = $state(false);
  function chooseOutput(id: string | null) {
    navigator.vibrate?.(8);
    setDevice({ output: id });
    choosingOutput = false;
  }
  // My order: shown at once, told to the app (it answers with all its settings).
  function setOrdered(on: boolean) {
    navigator.vibrate?.(8);
    appInfo.ordered = on;
    setDevice({ ordered: on });
  }
  let orderList = $state<HTMLElement>();
  const reorder = new Reorder(
    () => orderList,
    () => appInfo.known.length,
    (from, to) => {
      const k = [...appInfo.known];
      const [o] = k.splice(from, 1);
      k.splice(to, 0, o);
      appInfo.known = k;
      setDevice({ order: k.map((x) => x.id) });
    },
  );
  const connected = (id: string) => appInfo.outputs.some((o) => o.id === id);
  const FORGET = { icon: 'trash', label: 'Forget', danger: true } as const;
  function forget(id: string) {
    setTimeout(() => {
      appInfo.known = appInfo.known.filter((o) => o.id !== id);
      setDevice({ forget: id });
    }, 200);
    return 'away' as const;
  }

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

  // Following (0.10; the owner, 2026-10-30): this device's player listening along to another user's, who lets
  // others follow them; at the sheet's foot, letting others follow one, and those who do, each accepted at once or not.
  void loadFollow();
  const followed = $derived(followingNow());
  async function followTap(user: string, player: string) {
    if (!me) return;
    navigator.vibrate?.(8);
    const on = followed?.user === user && followed.player === player;
    try {
      await (on ? stopFollowing(me) : followOn(me, user, player));
      onclose();
    } catch (e) {
      toast.fail(e, on ? "It didn't stop following" : "It didn't follow");
    }
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
  title={asking ? asking.name : choosingOutput ? 'This device' : 'Play on'}
  images={asking ? (asking.track ? [coverUrl(asking.track)] : []) : src ? [src] : []}
  chips={asking ? (asking.track ? [asking.track.name] : []) : choosingOutput ? [] : [shown]}
  step={asking ? 'ask' : choosingOutput ? 'outputs' : 'menu'}
  onback={asking ? () => (asking = null) : choosingOutput ? () => (choosingOutput = false) : null}
  {onclose}
>
  <div role="menu">
    {#if asking}
      {@const name = asking.name}
      <MenuEntry label="Take over" onclick={() => choose(`player:${name}`, moveTo)} />
      <MenuEntry label="Don't take over" onclick={() => choose(`player:${name}`, joinTo)} />
    {:else if choosingOutput}
      <!-- One choice: the chosen one ticked, the others plain (no group's +). -->
      <MenuEntry label="Automatic" checked={appInfo.output === null ? true : null} onclick={() => chooseOutput(null)} />
      <MenuEntry icon="order" label="My order" switched={appInfo.ordered} onclick={() => setOrdered(!appInfo.ordered)} />
      {#if appInfo.ordered}
        <ol class="order" class:dragging={!!reorder.drag} bind:this={orderList}>
          {#each appInfo.known as o, i (o.id)}
            {@const on = connected(o.id)}
            <li class:lifted={reorder.drag?.from === i} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
              {#if !on}<SwipeBehind left={FORGET} />{/if}
              <div class="out" use:swipe={{ enabled: !on && !reorder.drag, left: () => forget(o.id) }}>
                <button class="pick" class:chosen={appInfo.output === o.id} disabled={!on} onclick={() => chooseOutput(o.id)}
                  ><Icon name={OUT_ICON[o.type]} size={24} /><span>{outName(o)}</span>{#if appInfo.output === o.id}<Icon name="check" size={20} />{/if}</button
                >
                <button
                  class="grip"
                  aria-label={`Drag ${outName(o)} to reorder`}
                  data-sheet-drag="off"
                  onpointerdown={(e) => reorder.down(e, i)}
                  onpointermove={reorder.move}
                  onpointerup={reorder.end}
                  onpointercancel={reorder.end}><Icon name="order" size={24} /></button
                >
              </div>
            </li>
          {/each}
        </ol>
      {:else}
        {#each appInfo.outputs as o (o.id)}
          <MenuEntry icon={OUT_ICON[o.type]} label={outName(o)} checked={appInfo.output === o.id ? true : null} onclick={() => chooseOutput(o.id)} />
        {/each}
      {/if}
    {:else}
      {#if canPlayHere}
        <div use:hold={{ start: () => me && held(me), end: () => {}, enabled: groupable(me), ms: HOLD_MS }}>
          <MenuEntry
            label="This device"
            checked={here || (me !== null && group.includes(me))}
            badge={me !== null && group.length > 1 && group[0] === me ? 'crown' : null}
            badgeLabel="Leads the group"
            onmark={groupable(me) ? () => me && held(me) : null}
            markLabel={me !== null && group.includes(me) ? 'Take out of the group' : 'Add to the group'}
            plus={false}
            onclick={() => pick('local')}
          />
        </div>
        {#if sounding}
          <MenuEntry
            icon={sounds ? OUT_ICON[sounds.type] : 'speaker'}
            label={sounds ? outName(sounds) : 'Automatic'}
            disabled={following}
            onclick={() => (choosingOutput = true)}
          />
        {/if}
      {/if}
      {#each players as name (name)}
        {@const now = playing(name)}
        {@const owner = playOn.players.find((p) => p.name === name)?.owner}
        <div use:hold={{ start: () => held(name), end: () => {}, enabled: groupable(name), ms: HOLD_MS }}>
          <MenuEntry
            label={owner ? `${name} · ${owner}'s` : name}
            sub={now?.text ?? null}
            subIcon={now ? (now.playing ? 'play' : 'pause') : null}
            checked={drivenName() === name || group.includes(name)}
            badge={group.length > 1 && group[0] === name ? 'crown' : null}
            badgeLabel="Leads the group"
            onmark={groupable(name) ? () => held(name) : null}
            markLabel={group.includes(name) ? 'Take out of the group' : 'Add to the group'}
            plus={false}
            onclick={() => pick(`player:${name}`)}
          />
        </div>
      {/each}
      <!-- The server's own player; not when what plays there is one of Cratarr's players listed above (the same one twice). -->
      {#if !playOn.players.some((p) => p.name === remoteName)}
        <MenuEntry label={remoteName} checked={!here && drivenName() === null} plus={false} onclick={() => pick('remote')} />
      {/if}
      <!-- Others' players, to follow on this device's (who let others follow them; the one followed ticked). -->
      {#if followOffered() && me}
        {#each follow.leaders as l (l.user)}
          {#each l.players as pl (pl.name)}
            <MenuEntry
              label={`Follow ${l.user} on ${pl.name}`}
              sub={pl.track ? (pl.track.artist ? `${pl.track.name} · ${pl.track.artist}` : pl.track.name) : null}
              subIcon={pl.track ? (pl.playing ? 'play' : 'pause') : null}
              checked={followed?.user === l.user && followed.player === pl.name}
              plus={false}
              onclick={() => followTap(l.user, pl.name)}
            />
          {/each}
        {/each}
      {/if}
      {#if group.length > 1}<GroupVolume names={group} />
      {:else if !inApp && here && playOn.self}<GroupVolume names={[playOn.self]} />{/if}
      <!-- At its foot: others may follow one, and those who do, each one's tracks asked for accepted at once or not. -->
      {#if followOffered() && mine.known}
        <MenuEntry icon="users" label="Let others follow me" switched={mine.followable} onclick={() => void setFollowable(!mine.followable)} />
        {#if mine.followable}
          {#each follow.followers as f (f.user)}
            {@const on = mine.autoAccept.includes(f.user)}
            <MenuEntry icon="thumbsUp" label={`Always accept ${f.user}`} switched={on} onclick={() => void setAutoAccept(f.user, !on)} />
          {/each}
        {/if}
      {/if}
    {/if}
  </div>
</MenuSheet>

<style>
  /* My order's rows: a menu entry's look (56 px, the icon in the accent), the handle at the end, as the queue's. */
  .order {
    position: relative;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .order li {
    position: relative;
  }

  .order.dragging li {
    transition: transform 200ms ease;
  }

  .order.dragging li.lifted {
    z-index: 1;
    transition: none;
  }

  .out {
    display: flex;
    align-items: center;
    background: var(--surface);
  }

  .pick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 56px;
    padding: 0 8px 0 24px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
    cursor: pointer;
  }

  .pick span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pick :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .pick.chosen {
    font-weight: 600;
  }

  .pick:disabled {
    color: color-mix(in srgb, var(--text) 38%, transparent);
    cursor: default;
  }

  .pick:disabled :global(svg) {
    opacity: 0.5;
  }

  .grip {
    display: grid;
    place-items: center;
    width: 48px;
    height: 56px;
    margin-right: 12px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    touch-action: none;
    cursor: grab;
  }
</style>
