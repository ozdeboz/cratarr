<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"), its administrators' since several users (decision 0051):
  // whether every request needs a sign-in (decision 0043); the directory. Switching the requirement asks for the
  // password again, in a sheet (the settings' North Star, 2026-10-19). Acts at once. API keys are with what uses them,
  // Home Assistant's under Connections (the owner, 2026-10-24). The accounts are all in Users (0051, amended
  // 2026-10-19: Cratarr's fallback administrator is one of them); each one's own password and sign-ins are under
  // Account, the address apps take too.
  import { getSignInRequired, setSignInRequired } from '../api';
  import DirectoryPart from './DirectoryPart.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  const REQUIRED_MORE =
    'On, every page and API request needs a sign-in, an API key or a signed link, on the home network too. Off, Cratarr asks none itself: a proxy in front of it may. Subsonic apps always sign in with their name and password.\n\nSwitching it asks for the password again.';

  let required = $state<boolean | null>(null);
  let error = $state<string | null>(null);

  getSignInRequired().then(
    (r) => (required = r.required),
    () => (required = null),
  );

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  // The requirement: flipping it asks for the password, then switches.
  let switching = $state<boolean | null>(null);
  let switchPassword = $state('');
  let busy = $state(false);
  async function confirmSwitch() {
    if (switching === null) return;
    error = null;
    busy = true;
    try {
      required = (await setSignInRequired(switching, switchPassword)).required;
      switching = null;
    } catch (e) {
      failed(e);
    } finally {
      switchPassword = '';
      busy = false;
    }
  }
</script>

{#if required !== null}
  <Group title="Sign-in">
    <Item
      name="Sign-in required everywhere"
      more={REQUIRED_MORE}
      disabled={busy}
      toggle={{ checked: switching ?? required, onchange: (on) => (switching = on === required ? null : on) }}
    />
  </Group>
{/if}

{#if error && switching === null}<Group><Item lead="bad" name={error} /></Group>{/if}
<DirectoryPart />

{#if switching !== null}
  <FormSheet
    title={switching ? 'Sign-in required everywhere' : 'Sign-in not required'}
    submitLabel={switching ? 'Switch on' : 'Switch off'}
    canSubmit={!!switchPassword}
    {busy}
    {error}
    onsubmit={confirmSwitch}
    onclose={() => ((switching = null), (switchPassword = ''), (error = null))}
  >
    <!-- svelte-ignore a11y_autofocus -->
    <label>Password<input type="password" bind:value={switchPassword} autocomplete="current-password" autofocus /></label>
  </FormSheet>
{/if}

