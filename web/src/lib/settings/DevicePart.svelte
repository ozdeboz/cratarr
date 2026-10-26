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
  import { isPhone, setLevelling } from '../localPlayer.svelte';
  import OutputSheet from '../OutputSheet.svelte';
  import { outputName } from '../playOn.svelte';
  import { allowDeviceMusic, askFolders, device, deviceMusicSettings, setFolder } from '../deviceMusic.svelte';
  import { plural } from '../insights.svelte';
  import { switchable } from '../deviceMusic.svelte';
  import { prefs, RAMP_MS, type StartStop } from '../prefs.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
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
  // Start and stop: the browser's own (prefs), or the app's player's (told to the app, as Level the volume).
  const MODES: { value: StartStop['mode']; label: string }[] = [
    { value: 'off', label: 'Off' },
    { value: 'vinyl', label: 'Vinyl' },
    { value: 'fade', label: 'Fade' },
  ];
  const STARTSTOP_MORE =
    "Vinyl: play and pause speed up from still and slow down to still, the pitch following, as a turntable. Fade: they fade in and out. Over the ramp's time. A track running into the next stays seamless.";
  const ss = $derived(inApp ? appInfo.startStop : prefs.startStop);
  function setSS(change: Partial<StartStop>) {
    const next = { ...ss, ...change };
    if (inApp) {
      appInfo.startStop = next;
      setDevice({ startStop: next });
    } else prefs.startStop = next;
  }
  let rampOpen = $state(false);
  let rampDrag = $state<number | null>(null);
</script>

<Group title="Playing">
  <Item name="Player name" value={name} onclick={() => (naming = true)} />
  <Item name="Where music plays" value={outputName()} onclick={() => (where = true)} />
  <Item
    name="Level the volume"
    line="Every track about as loud as the next"
    more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has).\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
    toggle={{ checked: inApp ? appInfo.levelling : prefs.levelling, onchange: setLevel }}
  />
  <!-- Start and stop (the owner, 2026-10-25): play and pause at once, as a turntable or fading, over a ramp; next and
       previous too where asked. Tape rewind waits for the real thing, the track played backwards (the owner,
       2026-10-26: hidden until then). -->
  <Item name="Start and stop" more={STARTSTOP_MORE}><Seg label="Start and stop" options={MODES} bind:value={() => ss.mode, (v) => setSS({ mode: v })} /></Item>
  {#if ss.mode !== 'off'}
    <Item name="Ramp" value={`${(ss.ms / 1000).toFixed(1)} s`} onclick={() => (rampOpen = true)} />
    <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
  {/if}
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
{#if rampOpen}
  <MenuSheet label="Ramp" title="Ramp" chips={[MODES.find((m) => m.value === ss.mode)?.label ?? '']} onclose={() => (rampOpen = false)}>
    <div class="ramp">
      <SettingRow label="Ramp">
        <input
          type="range"
          use:panSafe
          min={RAMP_MS.min}
          max={RAMP_MS.max}
          step="100"
          value={rampDrag ?? ss.ms}
          aria-label="Ramp"
          oninput={(e) => (rampDrag = Number(e.currentTarget.value))}
          onchange={(e) => {
            setSS({ ms: Number(e.currentTarget.value) });
            rampDrag = null;
          }}
        />
        <output>{((rampDrag ?? ss.ms) / 1000).toFixed(1)} s</output>
      </SettingRow>
    </div>
  </MenuSheet>
{/if}

<style>
  /* The ramp's sheet, as Radio's After each track. */
  .ramp {
    padding: 0 16px 16px;
  }

  .ramp input {
    width: 11em;
    max-width: 100%;
    accent-color: var(--accent);
  }

  .ramp output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
