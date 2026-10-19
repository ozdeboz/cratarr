<script lang="ts">
  // Authentication (the owner, 2026-10-16: not "Subsonic"), its administrators' since several users (decision 0051):
  // Cratarr's own account, an administrator apart from the directory; whether every request needs a sign-in (decision
  // 0043); the API keys for what cannot sign in on a page (Home Assistant, scripts), each acting for its owner, only
  // reading or also in control, each to revoke; the directory. Switching the requirement and making a key ask for the
  // password again, in a sheet (the settings' North Star, 2026-10-19). Acts at once. Each user's sign-ins are under
  // Account and Users; the address apps take is under Account.
  import { getApiKeys, getSignInRequired, getUsers, newApiKey, revokeApiKey, setSignInRequired, type ApiKey, type KeyScope } from '../api';
  import { me } from '../me.svelte';
  import Seg from '../Seg.svelte';
  import DirectoryPart from './DirectoryPart.svelte';
  import { ed } from './editor.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { when } from './when';

  const AUTH_MORE = "Cratarr's own administrator account, apart from the directory. Sign in to these pages with it. Subsonic apps take Cratarr's address with this account's own name and password (never a directory's).";
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
  // On opening, and after a new password.
  $effect(() => {
    if (!ed.sub.busy) load();
  });

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

  // The password: the current one and a new one, in a sheet.
  let changing = $state(false);
  async function changePassword() {
    await ed.changePassword();
    if (!ed.sub.current && !ed.sub.password) changing = false;
  }
  // Before there is an account: its name and password, typed in their sheets, then made.
  let typing = $state<'user' | 'password' | null>(null);
  const keyLine = (k: ApiKey) => [k.owner, k.scope === 'read' ? 'Read only' : null, k.last_used ? when(k.last_used) : 'Not used yet'].filter(Boolean).join(' · ');
</script>

{#if ed.sub.accounts}
  {#if ed.sub.accounts.length === 0}
    <Group title="Administrator account" more={AUTH_MORE}>
      <Item name="Username" line={ed.sub.user || 'Not set'} onclick={() => (typing = 'user')} />
      <Item name="Password" line={ed.sub.password ? 'Set' : 'Not set'} onclick={() => (typing = 'password')} />
      <Item name="Create account" kind="add" disabled={ed.sub.busy || !ed.sub.user.trim() || !ed.sub.password} busy={ed.sub.busy} onclick={ed.createAccount} />
    </Group>
  {:else}
    <Group title="Administrator account" more={AUTH_MORE}>
      <Item name="Account" value={ed.sub.accounts.join(', ')} />
      <Item name="Password" value="Change" onclick={() => (changing = true)} />
      {#if required !== null}
        <Item
          name="Sign-in required everywhere"
          more={REQUIRED_MORE}
          disabled={busy}
          toggle={{ checked: switching ?? required, onchange: (on) => (switching = on === required ? null : on) }}
        />
      {/if}
    </Group>

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
  {/if}
{/if}

{#if typing}
  <ValueSheet
    title={typing === 'user' ? 'Username' : 'Password'}
    type={typing === 'user' ? 'text' : 'password'}
    value={typing === 'user' ? ed.sub.user : ed.sub.password}
    onsave={(v) => {
      if (typing === 'user') ed.sub.user = v;
      else ed.sub.password = v;
    }}
    onclose={() => (typing = null)}
  />
{/if}
{#if changing}
  <FormSheet
    title="Change password"
    submitLabel="Change"
    canSubmit={!!ed.sub.current && !!ed.sub.password}
    busy={ed.sub.busy}
    onsubmit={changePassword}
    onclose={() => ((changing = false), (ed.sub.current = ''), (ed.sub.password = ''))}
  >
    <label>Current password<input type="password" bind:value={ed.sub.current} autocomplete="current-password" /></label>
    <label>New password<input type="password" bind:value={ed.sub.password} autocomplete="new-password" /></label>
  </FormSheet>
{/if}
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
