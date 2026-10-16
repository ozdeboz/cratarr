<script lang="ts">
  // This device (decision 0031): the app's own settings, this device's only; they act at once. How it keeps music
  // (quality, mobile data, the budget) is under Sync (decision 0038), how it reaches Cratarr under Network.
  import Switch from '../Switch.svelte';
  import { allowBackground, appInfo, inApp, setDevice } from '../app.svelte';
</script>

{#if inApp}
  <section aria-labelledby="device-h" class="device">
    <h2 id="device-h">{appInfo.player ?? 'This device'}</h2>
    <p class="lede">This device only.</p>
    <label class="pref">
      <span class="l">Player name</span>
      <input type="text" value={appInfo.player ?? ''} onchange={(e) => e.currentTarget.value.trim() && setDevice({ player: e.currentTarget.value.trim() })} />
    </label>
    <div class="pref">
      <Switch label="Stay ready" hint="Keeps listening for Play On while idle" checked={appInfo.stayReady} onchange={(on) => setDevice({ stayReady: on })} />
    </div>
    <div class="pref">
      <span class="l">Background <small>{appInfo.batteryAllowed ? 'Allowed to run' : 'Android may stop Cratarr'}</small></span>
      {#if !appInfo.batteryAllowed}<button onclick={allowBackground}>Allow</button>{/if}
    </div>
  </section>
{/if}
