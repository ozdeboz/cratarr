<script lang="ts">
  // Network (decided 2026-10-16, after Finamp's Network settings): how this device reaches Cratarr. The public address,
  // and the local network address, used while "Prefer local network" is on and it answers (at home, or over a VPN that
  // carries only the local network); the app looks again at every change of network. The address in use, and a test of
  // both. Signing in away from home is Cratarr's sign-in page (decision 0039), not a setting here. This device's own; it
  // acts at once. Only in the app.
  import { toast } from '../toast.svelte';
  import { appInfo, inApp, networkTest, setDevice, testNetwork } from '../app.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';
  import ValueSheet from './ValueSheet.svelte';

  // Its Reset: Prefer local network off; the addresses stay (lib/settings/reset).
  offerReset(
    () => (inApp && appInfo.addresses ? 'Prefer local network back to off. The addresses stay.' : null),
    () => setDevice({ preferLocal: false }),
  );

  /** An address as typed (the app tries https:// and then http:// when no scheme is typed). The public address is
   *  never cleared: the app needs it. One the app does not take leaves the one before, and a row names it. */
  function changed(typed: string, which: 'public' | 'local') {
    if (which === 'public' ? typed && typed !== appInfo.publicAddress : typed !== (appInfo.localAddress ?? '')) {
      setDevice(which === 'public' ? { public: typed, server: typed } : { local: typed });
    }
  }
  let editing = $state<'public' | 'local' | null>(null);

  // The test's answer, Finamp's way; only what failed is named (this device not signed in as that).
  const result = $derived.by(() => {
    if (!networkTest.done) return null;
    const { local, public: pub, signedOut } = networkTest;
    const pubFailed = signedOut ? 'Not signed in' : 'Public address not reachable';
    if (local === null) return pub ? 'Reachable' : signedOut ? 'Not signed in' : 'Not reachable';
    if (local && pub) return 'Both reachable';
    if (!local && !pub) return signedOut ? 'Local network address not reachable · not signed in' : 'Neither reachable';
    return local ? pubFailed : 'Local network address not reachable';
  });
  // The answer to a button is a toast (the owner, 2026-10-26: "a subrow is a horrible place for a result from clicking
  // a button"), as a connection's Check: good news plainly, anything not reached as an error.
  let asked = false;
  function test() {
    asked = true;
    testNetwork();
  }
  $effect(() => {
    const r = result;
    if (!asked || networkTest.testing || !r) return;
    asked = false;
    const good = r === 'Reachable' || r === 'Both reachable';
    toast.show({ text: `${r}.`, tone: good ? undefined : 'error' });
  });
</script>

{#if inApp}
  <Group>
    {#if appInfo.refused}<Item lead="bad" name="Cratarr does not answer" line={appInfo.refused} tone="bad" wrap />{/if}
    <Item name="Public address" line={appInfo.publicAddress ?? 'Not set'} wrap onclick={() => (editing = 'public')} />
    {#if appInfo.addresses}
      <Item
        name="Prefer local network"
        line="The local network address when it answers"
        more="On, the app uses the local network address whenever it answers (at home, or over a VPN that carries only the local network), and the public address otherwise. It looks again at every change of network."
        toggle={{ checked: appInfo.preferLocal, onchange: (on) => setDevice({ preferLocal: on }) }}
      />
      <!-- Only while it is used (as Finamp shows a setting's deeper ones only while they apply); kept while off. -->
      {#if appInfo.preferLocal}
        <Item
          name="Local network address"
          line={appInfo.localAddress ? (appInfo.localRefused ? `${appInfo.localAddress} · refused the sign-in` : appInfo.localAddress) : 'Not set'}
          tone={appInfo.localRefused ? 'bad' : null}
          wrap
          onclick={() => (editing = 'local')}
        />
      {/if}
      <Item name="Active address" line={appInfo.inUse ?? ''} wrap />
      <Item name="Test both connections">
        <button disabled={networkTest.testing} aria-busy={networkTest.testing} onclick={test}>Test</button>
      </Item>
    {/if}
  </Group>
{/if}

{#if editing}
  {@const which = editing}
  <ValueSheet
    title={which === 'public' ? 'Public address' : 'Local network address'}
    type="url"
    value={(which === 'public' ? appInfo.publicAddress : appInfo.localAddress) ?? ''}
    placeholder={which === 'local' ? 'http://192.168.1.5:8081' : ''}
    allowEmpty={which === 'local'}
    onsave={(v) => changed(v, which)}
    onclose={() => (editing = null)}
  />
{/if}
