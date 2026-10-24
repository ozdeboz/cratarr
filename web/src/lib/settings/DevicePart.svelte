<script lang="ts">
  // This device (decision 0031): the app's own settings, this device's only; they act at once. How it keeps music
  // (quality, mobile data, the budget) is under Sync (decision 0038), how it reaches Cratarr under Network. In a
  // browser: its name as a Play On player (lib/playOnTarget), this browser's only.
  // Local music (decision 0052, local mode): Android's permission while it is not given, then the folders Android found
  // music in, each with its track count and a switch; local mode reads only those on, and with one on the switcher shows.
  // How it plays (the owner, 2026-10-24: a play affair, kept with the device): where its music plays (the device
  // sheet, lib/OutputSheet), and levelling (decision 0035: in the app its own player's, here this browser's). On a
  // phone's browser: Lossless, off by default (lossless tracks stream as Opus there, the owner 2026-10-24).
  import { allowBackground, appInfo, inApp, setDevice } from '../app.svelte';
  import { isPhone, setLevelling } from '../localPlayer.svelte';
  import OutputSheet from '../OutputSheet.svelte';
  import { outputName } from '../playOn.svelte';
  import { allowDeviceMusic, askFolders, device, setFolder } from '../deviceMusic.svelte';
  import { plural } from '../insights.svelte';
  import { prefs } from '../prefs.svelte';
  import { defaultBrowserName } from '../playOnTarget.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';
  import ValueSheet from './ValueSheet.svelte';

  offerReset(
    () => (inApp ? 'Stay ready back to off.' : null),
    () => setDevice({ stayReady: false }),
  );

  $effect(() => {
    if (inApp && appInfo.deviceMusic === 'granted') askFolders();
  });
  let naming = $state(false);
  let where = $state(false);
  const setLevel = (on: boolean) => (inApp ? setDevice({ levelling: on }) : setLevelling(on));
  const name = $derived(inApp ? (appInfo.player ?? '') : prefs.browserName || defaultBrowserName());
  function rename(v: string) {
    if (!v) return;
    if (inApp) setDevice({ player: v });
    else prefs.browserName = v;
  }
</script>

<Group title={inApp ? 'This device' : 'This browser'}>
  <Item name="Player name" value={name} onclick={() => (naming = true)} />
  <Item name="Where music plays" value={outputName()} onclick={() => (where = true)} />
  <Item
    name="Level the volume"
    line="Every track about as loud as the next"
    more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has).\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
    toggle={{ checked: inApp ? appInfo.levelling : prefs.levelling, onchange: setLevel }}
  />
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
    <Item
      name="Background"
      line={appInfo.batteryAllowed ? 'Allowed to run' : 'Android may stop Cratarr'}
      tone={appInfo.batteryAllowed ? null : 'warn'}
      more="Android may stop an app in the background to save battery. Allowed, Cratarr keeps playing and stays ready with the screen off."
    >
      {#if !appInfo.batteryAllowed}<button onclick={allowBackground}>Allow</button>{/if}
    </Item>
  {/if}
</Group>

{#if inApp}
  <Group title="Local music">
    {#if appInfo.deviceMusic !== 'granted'}
      <Item
        name="This device's music"
        line={appInfo.deviceMusic === 'denied' ? 'Not allowed' : 'Not read'}
        tone={appInfo.deviceMusic === 'denied' ? 'warn' : null}
        more="Allowed, the app finds the music on this device, and the folders you turn on here play in Local, beside Cratarr."
      >
        <button onclick={allowDeviceMusic}>Allow</button>
      </Item>
    {:else if device.folders?.length}
      {#each device.folders as f (f.path)}
        <Item name={f.name} line={plural(f.tracks, 'track')} toggle={{ checked: f.on, onchange: (on) => setFolder(f.path, on) }} />
      {/each}
    {:else if device.folders}
      <Item name="No music found on this device" />
    {/if}
  </Group>
{/if}

{#if where}
  <OutputSheet onclose={() => (where = false)} />
{/if}
{#if naming}
  <ValueSheet title="Player name" value={name} placeholder={inApp ? '' : defaultBrowserName()} allowEmpty={false} onsave={rename} onclose={() => (naming = false)} />
{/if}
