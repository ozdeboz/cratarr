<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"), its administrators' since several users (decision 0051):
  // whether every request needs a sign-in (decision 0043); the API keys for what cannot sign in on a page (Home
  // Assistant, scripts), each acting for its owner, only reading or also in control, each to revoke; the directory.
  // Switching the requirement and making a key ask for the password again, in a sheet (the settings' North Star,
  // 2026-10-19). Acts at once. The accounts are all in Users (0051, amended 2026-10-19: Cratarr's fallback administrator
  // is one of them); each one's own password and sign-ins are under Account, the address apps take too.
  import { getApiKeys, getSignInRequired, getUsers, newApiKey, revokeApiKey, setSignInRequired, type ApiKey, type KeyScope } from '../api';
  import { me } from '../me.svelte';
  import Seg from '../Seg.svelte';
  import DirectoryPart from './DirectoryPart.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { when } from './when';

  const REQUIRED_MORE =
    'On, every page and API request needs a sign-in, an API key or a signed link, on the home network too. Off, Cratarr asks none itself: a proxy in front of it may. Subsonic apps always sign in with their name and password.\n\nSwitching it asks for the password again.';
  const KEYS_MORE = 'For what cannot sign in on a page: Home Assistant, scripts. A key shows once, when it is made; revoking one ends it at once.';
  const SCOPES: { value: KeyScope; label: string }[] = [
    { value: 'control', label: 'Control' },
    { value: 'read', label: 'Read only' },
  ];

  let keys = $state<ApiKey[]>([]);
  let required = $state<boolean | null>(null);
  let error = $state<string | null>(null);

  // Whom a key can act for: every user.
  let owners = $state<string[]>([]);
  const load = () => {
    getUsers().then(
      (u) => (owners = u.filter((x) => x.active).map((x) => x.name)),
      () => (owners = []),
    );
    getApiKeys().then(
      (r) => (keys = r.keys),
      () => (keys = []),
    );
    getSignInRequired().then(
      (r) => (required = r.required),
      () => (required = null),
    );
  };
  load();

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

  // A new key: its name, whom it acts for (oneself unless chosen), what it may do, and the password; the key is shown
  // here once.
  let making = $state(false);
  let keyName = $state('');
  let keyOwner = $state('');
  let keyScope = $state<KeyScope>('control');
  let keyPassword = $state('');
  let made = $state<{ name: string; key: string } | null>(null);
  async function makeKey() {
    error = null;
    busy = true;
    try {
      const k = await newApiKey(keyName.trim(), keyPassword, keyOwner || undefined, keyScope);
      made = { name: k.name, key: k.key };
      keyName = '';
      making = false;
      load();
    } catch (e) {
      failed(e);
    } finally {
      keyPassword = '';
      busy = false;
    }
  }

  async function revoke(k: ApiKey) {
    error = null;
    try {
      await revokeApiKey(k.id);
      if (made && made.name === k.name) made = null;
      load();
    } catch (e) {
      failed(e);
    }
  }

  const keyLine = (k: ApiKey) => [k.owner, k.scope === 'read' ? 'Read only' : null, k.last_used ? when(k.last_used) : 'Not used yet'].filter(Boolean).join(' · ');
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

<Group title="API keys" more={KEYS_MORE}>
  {#each keys as k (k.id)}
    <Item name={k.name} line={keyLine(k)}>
      <button onclick={() => revoke(k)}>Revoke</button>
    </Item>
  {/each}
  {#if made}
    {@const m = made}
    <Item lead="on" name={`${m.name}: shown only now`} line={m.key} wrap>
      <button onclick={() => navigator.clipboard?.writeText(m.key)}>Copy</button>
    </Item>
  {/if}
  <Item name="New key" kind="add" onclick={() => ((error = null), (making = true))} />
</Group>
{#if error && !making && switching === null}<Group><Item lead="bad" name={error} /></Group>{/if}
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
{#if making}
  <FormSheet title="New key" submitLabel="Make key" canSubmit={!!keyName.trim() && !!keyPassword} {busy} {error} onsubmit={makeKey} onclose={() => ((making = false), (keyPassword = ''), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Name<input type="text" bind:value={keyName} autocomplete="off" spellcheck="false" autofocus /></label>
    {#if owners.length > 1}
      <label
        >Acts for<select bind:value={keyOwner}>
          {#each owners as o (o)}<option value={o === me.name ? '' : o}>{o}</option>{/each}
        </select></label
      >
    {/if}
    <span class="scope"><Seg label="Scope" options={SCOPES} bind:value={keyScope} /></span>
    <label>Password<input type="password" bind:value={keyPassword} autocomplete="current-password" /></label>
  </FormSheet>
{/if}

<style>
  .scope {
    display: flex;
  }
</style>
