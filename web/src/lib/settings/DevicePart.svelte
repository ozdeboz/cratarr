<script lang="ts">
  // This device (decision 0031): the app's own settings, this device's only; they act at once. How it keeps music
  // (quality, mobile data, the budget) is under Sync (decision 0038), how it reaches Cratarr under Network. In a
  // browser: its name as a Play On player (lib/playOnTarget), this browser's only.
  // Local music (decision 0052, local mode): Android's permission while it is not given, then the folders Android found
  // music in, each with its track count and a switch; local mode reads only those on, and with one on the switcher shows.
  // How it plays (the owner, 2026-10-24: a play affair, kept with the device): where its music plays (the device
  // sheet, lib/OutputSheet), and levelling (decision 0035: in the app its own player's, here this browser's). On a
  // phone's browser: Lossless, off by default (lossless tracks stream as Opus there, the owner 2026-10-24).
  import { allowBackground, appInfo, backgroundSettings, inApp, setDevice } from '../app.svelte';
  import { startStopNow } from '../device.svelte';
  import { isPhone } from '../localPlayer.svelte';
  import LevelItems from './LevelItems.svelte';
  import VisualsAutoGroup from './VisualsAutoGroup.svelte';
  import OutputSheet from '../OutputSheet.svelte';
  import { outputName } from '../playOn.svelte';
  import { allowDeviceMusic, askFolders, device, deviceMusicSettings, setFolder } from '../deviceMusic.svelte';
  import { plural } from '../insights.svelte';
  import { localMode, switchable } from '../deviceMusic.svelte';
  import { prefs } from '../prefs.svelte';
  import { defaultBrowserName } from '../playOnTarget.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import StartStopSheet from '../StartStopSheet.svelte';

  offerReset(
    () => (inApp ? 'Stay ready back to off.' : null),
    () => setDevice({ stayReady: false }),
  );

  $effect(() => {
    if (inApp && appInfo.deviceMusic === 'granted') askFolders();
  });
  let naming = $state(false);
  let where = $state(false);
  const name = $derived(inApp ? (appInfo.player ?? '') : prefs.browserName || defaultBrowserName());
  function rename(v: string) {
    if (!v) return;
    if (inApp) setDevice({ player: v });
    else prefs.browserName = v;
  }
  let tapeOpen = $state(false);
</script>

<Group title="Playing">
  <Item name="Player name" value={name} onclick={() => (naming = true)} />
  <Item name="Where music plays" value={outputName()} onclick={() => (where = true)} />
  <!-- Level the volume (lib/settings/LevelItems: the same rows as the Level button's long press). -->
  <LevelItems />
  <!-- The tape (the owner, 2026-10-27: one page in order): Start and stop, the tape, its four motions, the deck, on its
       own page (lib/StartStopSheet, as the play button's long press opens it). -->
  {@const tape = startStopNow()}
  <Item name="Tape" value={[tape.mode === 'off' ? 'No effect' : tape.mode === 'vinyl' ? 'Vinyl' : 'Fade', tape.rewind ? 'winding' : null].filter(Boolean).join(' · ')} onclick={() => (tapeOpen = true)} />
  {#if !inApp && isPhone}
    <Item
      name="Lossless"
      line="More data and battery"
      more={"On a phone, lossless tracks stream as Opus: about a sixth of the data, and transparent to most ears.\n\nOn: the original files instead, from the next track."}
      toggle={{ checked: prefs.phoneLossless, onchange: (on) => (prefs.phoneLossless = on) }}
    />
  {/if}
  {#if inApp}
    <Item
      name="Stay ready"
      line="Keeps listening for Play On while idle"
      more="While nothing plays, the app keeps its connection to Cratarr up (a wake lock and a Wi-Fi lock), so Play On from Home Assistant or another device starts music here with the screen locked. It uses some battery."
      toggle={{ checked: appInfo.stayReady, onchange: (on) => setDevice({ stayReady: on }) }}
    />
    <!-- A switch (the owner, 2026-10-25: not an Allow button): on asks Android's dialog, off opens Android's list, where
         only the user takes it back; the switch follows what Android says when the app comes back. -->
    <Item
      name="Background"
      line={appInfo.batteryAllowed ? 'Allowed to run' : 'Android may stop Cratarr'}
      tone={appInfo.batteryAllowed ? null : 'warn'}
      more="Android may stop an app in the background to save battery. Allowed, Cratarr keeps playing and stays ready with the screen off. Turned off, Android's own settings open, where it is taken back."
      toggle={{ checked: appInfo.batteryAllowed, onchange: (on) => (on ? allowBackground() : backgroundSettings()) }}
    />
  {/if}
</Group>

<!-- The visuals by themselves (0.10): this device's, not in local mode (no visuals there, decision 0052). -->
{#if !localMode()}<VisualsAutoGroup />{/if}

{#if inApp}
  <Group title="Local music">
    <!-- A switch (the owner, 2026-10-25): on asks Android (or opens its settings, refused before), off opens Android's
         settings, where a permission is taken back; the folders under it once allowed. -->
    <Item
      name="This device's music"
      line={appInfo.deviceMusic === 'granted' ? null : appInfo.deviceMusic === 'denied' ? 'Not allowed' : 'Not read'}
      tone={appInfo.deviceMusic === 'denied' ? 'warn' : null}
      more="Allowed, the app finds the music on this device, and the folders you turn on here play in Local, beside Cratarr. Turned off, Android's own settings open, where it is taken back."
      toggle={{ checked: appInfo.deviceMusic === 'granted', onchange: (on) => (on ? allowDeviceMusic() : deviceMusicSettings()) }}
    />
    {#if appInfo.deviceMusic !== 'granted'}
      <!-- Nothing read yet. -->
    {:else if device.folders?.length}
      {#each device.folders as f (f.path)}
        <Item name={f.name} line={plural(f.tracks, 'track')} toggle={{ checked: f.on, onchange: (on) => setFolder(f.path, on) }} />
      {/each}
    {:else if device.folders}
      <Item name="No music found on this device" />
    {/if}
    {#if switchable()}
      <Item
        name="Mode switch in the bar"
        more="Cratarr or Local, over the bar's pages. Off, it is at the top of Settings."
        toggle={{ checked: prefs.modeSwitchInBar, onchange: (on) => (prefs.modeSwitchInBar = on) }}
      />
    {/if}
  </Group>
{/if}

{#if where}
  <OutputSheet onclose={() => (where = false)} />
{/if}
{#if naming}
  <ValueSheet title="Player name" value={name} placeholder={inApp ? '' : defaultBrowserName()} allowEmpty={false} onsave={rename} onclose={() => (naming = false)} />
{/if}
{#if tapeOpen}<StartStopSheet onclose={() => (tapeOpen = false)} />{/if}
