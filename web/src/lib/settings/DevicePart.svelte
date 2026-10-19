<script lang="ts">
  // This device (decision 0031): the app's own settings, this device's only; they act at once. How it keeps music
  // (quality, mobile data, the budget) is under Sync (decision 0038), how it reaches Cratarr under Network. In a
  // browser: its name as a Play On player (lib/playOnTarget), this browser's only.
  import { allowBackground, appInfo, inApp, setDevice } from '../app.svelte';
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

  let naming = $state(false);
  const name = $derived(inApp ? (appInfo.player ?? '') : prefs.browserName || defaultBrowserName());
  function rename(v: string) {
    if (!v) return;
    if (inApp) setDevice({ player: v });
    else prefs.browserName = v;
  }
</script>

<Group title={inApp ? 'This device' : 'This browser'}>
  <Item name="Player name" value={name} onclick={() => (naming = true)} />
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

{#if naming}
  <ValueSheet title="Player name" value={name} placeholder={inApp ? '' : defaultBrowserName()} allowEmpty={false} onsave={rename} onclose={() => (naming = false)} />
{/if}
