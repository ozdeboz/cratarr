<script lang="ts">
  // Account (decision 0051, several users): who is signed in and their role, read-only (no address: a browser's is in
  // its address bar, the app's under Network; the owner, 2026-10-24); their password: a Cratarr account's own (0051, amended 2026-10-19; it is its OpenSubsonic one too), or for a
  // directory's account its own password for OpenSubsonic apps (whose sign-in needs the password itself, which a
  // directory never gives); their devices (each plays only on its own) and their sign-ins, each to end. Acts at once.
  // Their Spotify (decision 0073; the owner, 2026-10-27: each connects here): a row opening its own page
  // (`thing`: `spotify`, lib/settings/SpotifyPart).
  import { changeOwnPassword, endMySession, removable, removeMyDevice, setAppPassword, type OwnDevice, type UserSession } from '../api';
  import { downloads, inApp } from '../app.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { loadMe, me } from '../me.svelte';
  import { signOut } from '../signOut.svelte';
  import { toast } from '../toast.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { when } from './when';
  import { openThing } from '../panel.svelte';
  import { loadSpotify, spotify } from '../spotify.svelte';
  import SpotifyPart from './SpotifyPart.svelte';

  let { thing = null }: { thing?: string | null } = $props();
  loadSpotify(true);
  const sp = $derived(spotify.state);
  const spotifyLine = $derived(
    !sp ? null : sp.live ? `Connected as ${sp.live.name}` : sp.imported ? 'From your export' : sp.client_id ? 'Not connected' : 'Not set up',
  );

  // This device's own Sign out, with who is signed in (the owner, 2026-10-29: where one looks for it): only where the
  // page came with a sign-in of its own (an API key's or the app's address alone has none to end). With downloads of
  // the account here, a sheet first: they stay hidden for its next sign-in, or go (decision 0101; lib/signOut).
  const signedInHere = $derived(!!me.sessions?.some((s) => s.current));
  let signingOut = $state(false);
  const out = (remove: boolean) => void signOut(remove);

  // A Cratarr account's own password: the current one and a new one. A directory's app password: the one they sign in
  // with and a new one.
  let changing = $state(false);
  let setting = $state(false);
  let current = $state('');
  let next = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  async function savePassword() {
    busy = true;
    error = null;
    try {
      await setAppPassword(current, next);
      await loadMe();
      setting = false;
      current = '';
      next = '';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function change() {
    busy = true;
    error = null;
    try {
      await changeOwnPassword(current, next);
      changing = false;
      current = '';
      next = '';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function end(s: UserSession) {
    try {
      await endMySession(s.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not sign it out');
    }
  }

  async function remove(d: OwnDevice) {
    try {
      await removeMyDevice(d.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not remove it');
    }
  }
</script>

{#if thing === 'spotify'}
  <SpotifyPart />
{:else}
<Group>
  {#if me.name}<Item name={me.name} line={me.admin ? 'Administrator' : 'User'} />{:else}<Item name={me.admin ? 'Administrator' : 'User'} />{/if}
  {#if me.name && me.directory === false}
    <Item name="Password" value="Change" onclick={() => ((error = null), (changing = true))} />
  {:else if me.name}
    <Item
      name="App password"
      more="OpenSubsonic apps sign in with your name and this password, never your directory password."
      value={me.app_password ? 'Set' : 'Not set'}
      onclick={() => ((error = null), (setting = true))}
    />
  {/if}
  {#if signedInHere}<Item name="Sign out" kind="danger" onclick={() => (inApp && downloads.any ? (signingOut = true) : out(false))} />{/if}
</Group>

<Group>
  <Item name="Spotify" line={spotifyLine} onclick={() => openThing('account', 'spotify')} />
</Group>

{#if me.devices?.length}
  <Group title="Devices">
    {#each me.devices as d (d.id)}
      <Item lead={d.online ? 'on' : 'off'} name={d.name} line={`${d.kind} · ${d.online ? 'online' : d.last_seen ? when(d.last_seen) : 'offline'}`}>
        {#if removable(d)}<button onclick={() => remove(d)}>Remove</button>{/if}
      </Item>
    {/each}
  </Group>
{/if}

{#if me.sessions?.length}
  <Group title="Signed in">
    {#each me.sessions as s (s.id)}
      <Item name={s.device} line={s.current ? 'This device' : when(s.last_used)}>
        {#if !s.current}<button onclick={() => end(s)}>Sign out</button>{/if}
      </Item>
    {/each}
  </Group>
{/if}

{#if signingOut}
  <MenuSheet label="Sign out" icon="user" title="Sign out?" onclose={() => (signingOut = false)}>
    <MenuEntry icon="user" label="Sign out" onclick={() => ((signingOut = false), out(false))} />
    <MenuEntry icon="trash" label="Remove this account's downloads" danger onclick={() => ((signingOut = false), out(true))} />
    <MenuEntry icon="close" label="Cancel" onclick={() => (signingOut = false)} />
  </MenuSheet>
{/if}

{/if}

{#if changing}
  <FormSheet title="Change password" submitLabel="Change" canSubmit={!!current && !!next} {busy} {error} onsubmit={change} onclose={() => ((changing = false), (current = ''), (next = ''), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Current password<input type="password" bind:value={current} autocomplete="current-password" autofocus /></label>
    <label>New password<input type="password" bind:value={next} autocomplete="new-password" /></label>
  </FormSheet>
{/if}
{#if setting}
  <FormSheet title="App password" submitLabel="Save" canSubmit={!!current && !!next} {busy} {error} onsubmit={savePassword} onclose={() => ((setting = false), (current = ''), (next = ''), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Your password<input type="password" bind:value={current} autocomplete="current-password" autofocus /></label>
    <label>App password<input type="password" bind:value={next} autocomplete="new-password" /></label>
  </FormSheet>
{/if}
