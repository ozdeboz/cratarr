<script lang="ts">
  // This device (decision 0031): the app's own settings, this device's only; they act at once.
  import Switch from '../Switch.svelte';
  import { allowBackground, appInfo, downloads, inApp, setDevice } from '../app.svelte';
  import { downloadedSize } from './editor.svelte';
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
    <label class="pref">
      <span class="l">Server</span>
      <input
        type="url"
        value={appInfo.server ?? ''}
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        onchange={(e) => {
          const server = e.currentTarget.value.trim();
          if (server && server !== appInfo.server) setDevice({ server });
        }}
      />
    </label>
    <label class="pref">
      <span class="l">Downloads <small>{downloads.tracks.length} tracks · {downloadedSize(downloads.bytes)}</small></span>
      <select value={appInfo.downloadQuality} onchange={(e) => setDevice({ downloadQuality: e.currentTarget.value })}>
        <option value="original">Originals</option>
        <option value="smaller">Smaller copies</option>
      </select>
    </label>
    <div class="pref">
      <Switch label="Download over mobile data" checked={appInfo.downloadOverMobile} onchange={(on) => setDevice({ downloadOverMobile: on })} />
    </div>
  </section>
{/if}
