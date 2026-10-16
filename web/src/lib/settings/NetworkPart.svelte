<script lang="ts">
  // Network (decided 2026-10-16, after Finamp's Network settings): how this device reaches Cratarr. The public address,
  // and the local network address, used while "Prefer local network" is on and it answers (at home, or over a VPN that
  // carries only the local network); the app looks again at every change of network. The address in use, and a test of
  // both. This device's own; it acts at once. Only in the app.
  import SettingRow from '../SettingRow.svelte';
  import Switch from '../Switch.svelte';
  import { appInfo, inApp, networkTest, setDevice, testNetwork } from '../app.svelte';

  /** An address field's change, sent as typed (the app adds the scheme when there is none). */
  function changed(e: Event & { currentTarget: HTMLInputElement }, which: 'public' | 'local') {
    const typed = e.currentTarget.value.trim();
    if (which === 'public') {
      // Never cleared: the app needs it. One that does not answer is not taken (the field shows the one kept again).
      if (typed && typed !== appInfo.publicAddress) setDevice({ public: typed, server: typed });
      else e.currentTarget.value = appInfo.publicAddress ?? '';
    } else if (typed !== (appInfo.localAddress ?? '')) {
      setDevice({ local: typed });
    }
  }

  // The test's answer, Finamp's way; only what failed is named.
  const result = $derived.by(() => {
    if (!networkTest.done) return null;
    const { local, public: pub } = networkTest;
    if (local === null) return pub ? 'Reachable' : 'Not reachable';
    if (local && pub) return 'Both reachable';
    if (!local && !pub) return 'Neither reachable';
    return local ? 'Public address not reachable' : 'Local network address not reachable';
  });
</script>

{#if inApp}
  <section aria-labelledby="network-h">
    <h2 id="network-h">Network</h2>
    <p class="lede">This device only.</p>
    <label class="pref">
      <span class="l">Public address</span>
      <input
        type="url"
        value={appInfo.publicAddress ?? ''}
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        onchange={(e) => changed(e, 'public')}
      />
    </label>
    {#if appInfo.addresses}
      <div class="pref">
        <Switch label="Prefer local network" checked={appInfo.preferLocal} onchange={(on) => setDevice({ preferLocal: on })} />
      </div>
      <label class="pref">
        <span class="l">Local network address</span>
        <input
          type="url"
          value={appInfo.localAddress ?? ''}
          placeholder="http://192.168.1.5:8081"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          onchange={(e) => changed(e, 'local')}
        />
      </label>
      <SettingRow label="Active address">
        <span class="active">{appInfo.inUse ?? ''}</span>
      </SettingRow>
      <SettingRow label="Test both connections" hint={networkTest.testing ? null : result}>
        <button disabled={networkTest.testing} aria-busy={networkTest.testing} onclick={testNetwork}>Test</button>
      </SettingRow>
    {/if}
  </section>
{/if}

<style>
  .active {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
